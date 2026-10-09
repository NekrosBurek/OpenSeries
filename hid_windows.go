package main

import (
	"errors"
	"strings"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

// Arctis 7+ dongle. Protocol from HeadsetControl: write {0x00,0xb0} on interface 3,
// reply is [reportID, 0xb0, status(1=offline), battery 0..4, charging(1)].
const (
	arctisPID   = "pid_220e"
	arctisIface = "mi_03"
)

var (
	hidGUID       = windows.GUID{Data1: 0x4D1E55B2, Data2: 0xF16F, Data3: 0x11CF, Data4: [8]byte{0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30}}
	hidDLL        = windows.NewLazySystemDLL("hid.dll")
	pGetPreparsed = hidDLL.NewProc("HidD_GetPreparsedData")
	pFreePrepared = hidDLL.NewProc("HidD_FreePreparsedData")
	pGetCaps      = hidDLL.NewProc("HidP_GetCaps")
)

type headset struct {
	Dongle   bool // USB receiver found
	Online   bool // headset powered on and paired
	Battery  int  // 0..100
	Charging bool
	Raw      []byte
}

var errNoReply = errors.New("no reply from headset")

func readHeadset() (headset, error) {
	paths, err := windows.CM_Get_Device_Interface_List("", &hidGUID, windows.CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
	if err != nil {
		return headset{}, err
	}
	var h headset
	for _, p := range paths {
		lp := strings.ToLower(p)
		if !strings.Contains(lp, "vid_1038&"+arctisPID+"&"+arctisIface) {
			continue
		}
		h.Dongle = true
		buf, err := hidQuery(p, []byte{0x00, 0xb0}, 0xb0)
		if err != nil {
			continue
		}
		h.Raw = buf
		h.Online = buf[2] != 0x01
		h.Battery = min(int(buf[3]), 4) * 25
		h.Charging = buf[4] == 0x01
		return h, nil
	}
	if !h.Dongle {
		return h, nil
	}
	return h, errNoReply
}

// hidQuery writes one output report and waits for an input report whose second byte is want.
func hidQuery(path string, req []byte, want byte) ([]byte, error) {
	p, _ := windows.UTF16PtrFromString(path)
	f, err := windows.CreateFile(p, windows.GENERIC_READ|windows.GENERIC_WRITE,
		windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE, nil, windows.OPEN_EXISTING, windows.FILE_FLAG_OVERLAPPED, 0)
	if err != nil {
		return nil, err
	}
	defer windows.CloseHandle(f)

	var prep uintptr
	if r, _, _ := pGetPreparsed.Call(uintptr(f), uintptr(unsafe.Pointer(&prep))); r == 0 {
		return nil, errors.New("HidD_GetPreparsedData failed")
	}
	var caps [32]uint16 // HIDP_CAPS: [2]=input len, [3]=output len
	pGetCaps.Call(prep, uintptr(unsafe.Pointer(&caps)))
	pFreePrepared.Call(prep)
	inLen, outLen := int(caps[2]), int(caps[3])
	if inLen < 5 || outLen < len(req) {
		return nil, errors.New("wrong HID collection")
	}

	out := make([]byte, outLen)
	copy(out, req)
	if _, err := overlapped(f, out, true, time.Second); err != nil {
		return nil, err
	}
	deadline := time.Now().Add(time.Second)
	in := make([]byte, inLen)
	for time.Now().Before(deadline) {
		n, err := overlapped(f, in, false, time.Until(deadline))
		if err != nil {
			return nil, err
		}
		if n >= 5 && in[1] == want {
			return in[:n], nil
		}
	}
	return nil, errNoReply
}

func overlapped(f windows.Handle, buf []byte, write bool, timeout time.Duration) (int, error) {
	ev, err := windows.CreateEvent(nil, 1, 0, nil)
	if err != nil {
		return 0, err
	}
	defer windows.CloseHandle(ev)
	ov := windows.Overlapped{HEvent: ev}
	if write {
		err = windows.WriteFile(f, buf, nil, &ov)
	} else {
		err = windows.ReadFile(f, buf, nil, &ov)
	}
	if err != nil && err != windows.ERROR_IO_PENDING {
		return 0, err
	}
	if ev, _ := windows.WaitForSingleObject(ev, uint32(timeout.Milliseconds())); ev != windows.WAIT_OBJECT_0 {
		windows.CancelIoEx(f, &ov)
		var n uint32
		windows.GetOverlappedResult(f, &ov, &n, true)
		return 0, errNoReply
	}
	var n uint32
	if err := windows.GetOverlappedResult(f, &ov, &n, false); err != nil {
		return 0, err
	}
	return int(n), nil
}
