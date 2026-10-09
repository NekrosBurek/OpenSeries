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
	Game     int // ChatMix dial, 0..100 each
	Chat     int
	Raw      []byte
}

var errNoReply = errors.New("no reply from headset")

func arctisPaths() []string {
	paths, _ := windows.CM_Get_Device_Interface_List("", &hidGUID, windows.CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
	var out []string
	for _, p := range paths {
		if strings.Contains(strings.ToLower(p), "vid_1038&"+arctisPID+"&"+arctisIface) {
			out = append(out, p)
		}
	}
	return out
}

func readHeadset() (headset, error) {
	var h headset
	for _, p := range arctisPaths() {
		h.Dongle = true
		buf, err := hidQuery(p, [][]byte{{0x00, 0xb0}}, 0xb0)
		if err != nil {
			continue
		}
		h.Raw = buf
		h.Online = buf[2] != 0x01
		h.Battery = min(int(buf[3]), 4) * 25
		h.Charging = buf[4] == 0x01
		h.Game, h.Chat = min(int(buf[5]), 100), min(int(buf[6]), 100)
		return h, nil
	}
	if !h.Dongle {
		return h, nil
	}
	return h, errNoReply
}

// hidSend writes commands followed by "save to headset" (0x09). Settings are write-only on this device.
func hidSend(cmds ...[]byte) error {
	err := errors.New("receiver not connected")
	for _, p := range arctisPaths() {
		if _, err = hidQuery(p, append(cmds, []byte{0x00, 0x09}), 0); err == nil {
			return nil
		}
	}
	return err
}

// hidQuery writes output reports, then (if want != 0) waits for an input report whose second byte is want.
func hidQuery(path string, reqs [][]byte, want byte) ([]byte, error) {
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
	if inLen < 7 || outLen < 2 {
		return nil, errors.New("wrong HID collection")
	}
	for _, req := range reqs {
		if len(req) > outLen {
			return nil, errors.New("command longer than output report")
		}
		out := make([]byte, outLen)
		copy(out, req)
		if _, err := overlapped(f, out, true, time.Second); err != nil {
			return nil, err
		}
	}
	if want == 0 {
		return nil, nil
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

func setSidetone(level int) error { return hidSend([]byte{0x00, 0x39, byte(level)}) } // 0..3

func setAutoOff(minutes int) error { return hidSend([]byte{0x00, 0xa3, byte(minutes)}) } // 0 = never, max 90

// setEQ takes 10 bands in dB (-12..+12, 0.5 steps).
func setEQ(bands [10]float32) error {
	cmd := []byte{0x00, 0x33}
	for _, b := range bands {
		cmd = append(cmd, byte(0x18+int(max(-12, min(12, b))*2)))
	}
	return hidSend(append(cmd, 0x00))
}
