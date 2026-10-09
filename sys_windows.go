package main

import (
	"fmt"
	"os"
	"strings"
	"unsafe"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"
)

const runKey = `Software\Microsoft\Windows\CurrentVersion\Run`

func autostart() bool {
	k, err := registry.OpenKey(registry.CURRENT_USER, runKey, registry.QUERY_VALUE)
	if err != nil {
		return false
	}
	defer k.Close()
	_, _, err = k.GetStringValue(appName)
	return err == nil
}

func setAutostart(on bool) error {
	k, err := registry.OpenKey(registry.CURRENT_USER, runKey, registry.SET_VALUE)
	if err != nil {
		return err
	}
	defer k.Close()
	if !on {
		return k.DeleteValue(appName)
	}
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	return k.SetStringValue(appName, `"`+exe+`" --tray`)
}

// ask shows a message box; with yesNo it returns true on Yes.
func ask(text string, yesNo bool) bool {
	t, _ := windows.UTF16PtrFromString(text)
	c, _ := windows.UTF16PtrFromString(appName)
	flags := uint32(windows.MB_ICONINFORMATION | windows.MB_SETFOREGROUND)
	if yesNo {
		flags |= windows.MB_YESNO
	}
	r, _ := windows.MessageBox(0, t, c, flags)
	return r == 6 // IDYES
}

type pnpDev struct {
	ID, Name string
	Problem  uint32 // 0 = OK, else Device Manager problem code (28 = no driver, 10 = can't start, ...)
}

// pnpDevices returns present device nodes belonging to the Arctis dongle.
func pnpDevices() []pnpDev {
	set, err := windows.SetupDiGetClassDevsEx(nil, "", 0, windows.DIGCF_ALLCLASSES|windows.DIGCF_PRESENT, 0, "")
	if err != nil {
		return nil
	}
	defer set.Close()
	var out []pnpDev
	for i := 0; ; i++ {
		d, err := set.EnumDeviceInfo(i)
		if err != nil {
			break
		}
		id, _ := set.DeviceInstanceID(d)
		if !strings.Contains(strings.ToLower(id), "vid_1038&"+arctisPID) {
			continue
		}
		name, _ := set.DeviceRegistryProperty(d, windows.SPDRP_FRIENDLYNAME)
		if name == nil {
			name, _ = set.DeviceRegistryProperty(d, windows.SPDRP_DEVICEDESC)
		}
		var status, problem uint32
		windows.CM_Get_DevNode_Status(&status, &problem, d.DevInst, 0)
		s, _ := name.(string)
		out = append(out, pnpDev{ID: id, Name: s, Problem: problem})
	}
	return out
}

type proc struct {
	Name string
	MB   int // working set, -1 if not readable (e.g. runs as a service)
}

var pGetMemInfo = windows.NewLazySystemDLL("kernel32.dll").NewProc("K32GetProcessMemoryInfo")

func steelseriesProcs() []proc {
	snap, err := windows.CreateToolhelp32Snapshot(windows.TH32CS_SNAPPROCESS, 0)
	if err != nil {
		return nil
	}
	defer windows.CloseHandle(snap)
	var out []proc
	e := windows.ProcessEntry32{Size: uint32(unsafe.Sizeof(windows.ProcessEntry32{}))}
	for err = windows.Process32First(snap, &e); err == nil; err = windows.Process32Next(snap, &e) {
		name := windows.UTF16ToString(e.ExeFile[:])
		if !strings.HasPrefix(strings.ToLower(name), "steelseries") {
			continue
		}
		p := proc{Name: name, MB: -1}
		if h, err := windows.OpenProcess(windows.PROCESS_QUERY_LIMITED_INFORMATION, false, e.ProcessID); err == nil {
			var m [10]uintptr // PROCESS_MEMORY_COUNTERS; [0] holds cb+PageFaultCount, [2] WorkingSetSize
			*(*uint32)(unsafe.Pointer(&m[0])) = uint32(unsafe.Sizeof(m))
			if r, _, _ := pGetMemInfo.Call(uintptr(h), uintptr(unsafe.Pointer(&m)), unsafe.Sizeof(m)); r != 0 {
				p.MB = int(m[2] >> 20)
			}
			windows.CloseHandle(h)
		}
		out = append(out, p)
	}
	return out
}

// restartDongle asks for elevation and restarts the USB receiver (same as unplug/replug).
func restartDongle(usbID string) error {
	verb, _ := windows.UTF16PtrFromString("runas")
	exe, _ := windows.UTF16PtrFromString("pnputil.exe")
	args, _ := windows.UTF16PtrFromString(fmt.Sprintf(`/restart-device "%s"`, usbID))
	return windows.ShellExecute(0, verb, exe, args, nil, windows.SW_HIDE)
}

var instance windows.Handle

const showEventName = `Local\` + appName + `.show`

// singleInstance returns false if another copy runs; that copy is told to show its window.
func singleInstance() bool {
	name, _ := windows.UTF16PtrFromString(`Local\` + appName)
	h, err := windows.CreateMutex(nil, false, name)
	if err == windows.ERROR_ALREADY_EXISTS {
		windows.CloseHandle(h)
		ev, _ := windows.UTF16PtrFromString(showEventName)
		if e, err := windows.OpenEvent(windows.EVENT_MODIFY_STATE, false, ev); err == nil {
			windows.SetEvent(e)
			windows.CloseHandle(e)
		}
		return false
	}
	instance = h
	return err == nil
}

func releaseInstance() { windows.CloseHandle(instance) }

// waitShow calls f each time a second copy is launched.
func waitShow(f func()) {
	name, _ := windows.UTF16PtrFromString(showEventName)
	ev, err := windows.CreateEvent(nil, 0, 0, name)
	if err != nil {
		return
	}
	for {
		windows.WaitForSingleObject(ev, windows.INFINITE)
		f()
	}
}
