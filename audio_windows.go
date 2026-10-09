package main

import (
	"fmt"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

// Raw COM vtable calls against MMDevice API + the undocumented IPolicyConfig
// (what every "set default device" tool uses; Windows has no public API for it).

const (
	flowOut = 0 // eRender
	flowIn  = 1 // eCapture
)

var (
	ole32             = windows.NewLazySystemDLL("ole32.dll")
	pCoCreateInstance = ole32.NewProc("CoCreateInstance")
	pPropVariantClear = ole32.NewProc("PropVariantClear")
	clsidEnumerator   = windows.GUID{Data1: 0xBCDE0395, Data2: 0xE52F, Data3: 0x467C, Data4: [8]byte{0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E}}
	iidEnumerator     = windows.GUID{Data1: 0xA95664D2, Data2: 0x9614, Data3: 0x4F35, Data4: [8]byte{0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6}}
	clsidPolicyConfig = windows.GUID{Data1: 0x870AF99C, Data2: 0x171D, Data3: 0x4F9E, Data4: [8]byte{0xAF, 0x0D, 0xE6, 0x3D, 0xF4, 0x0C, 0x2B, 0xC9}}
	iidPolicyConfig   = windows.GUID{Data1: 0xF8679F50, Data2: 0x850A, Data3: 0x41CF, Data4: [8]byte{0x9C, 0x72, 0x43, 0x0F, 0x29, 0x02, 0x90, 0xC8}}
	pkeyFriendlyName  = struct {
		fmtid windows.GUID
		pid   uint32
	}{windows.GUID{Data1: 0xA45C254E, Data2: 0xDF1C, Data3: 0x4EFD, Data4: [8]byte{0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 14}
)

type endpoint struct{ ID, Name string }

func comInit() { windows.CoInitializeEx(0, windows.COINIT_MULTITHREADED) }

func vcall(obj uintptr, idx uintptr, args ...uintptr) error {
	fn := *(*uintptr)(unsafe.Pointer(*(*uintptr)(unsafe.Pointer(obj)) + idx*unsafe.Sizeof(obj)))
	r, _, _ := syscall.SyscallN(fn, append([]uintptr{obj}, args...)...)
	if int32(r) < 0 {
		return fmt.Errorf("COM call %d: 0x%08x", idx, uint32(r))
	}
	return nil
}

func release(obj uintptr) { vcall(obj, 2) }

func create(clsid, iid *windows.GUID) (uintptr, error) {
	var obj uintptr
	r, _, _ := pCoCreateInstance.Call(uintptr(unsafe.Pointer(clsid)), 0, 0x17, uintptr(unsafe.Pointer(iid)), uintptr(unsafe.Pointer(&obj)))
	if int32(r) < 0 {
		return 0, fmt.Errorf("CoCreateInstance: 0x%08x", uint32(r))
	}
	return obj, nil
}

func takeString(p uintptr) string {
	s := windows.UTF16PtrToString((*uint16)(unsafe.Pointer(p)))
	windows.CoTaskMemFree(unsafe.Pointer(p))
	return s
}

// endpoints lists active devices for a flow.
func endpoints(flow int) ([]endpoint, error) {
	en, err := create(&clsidEnumerator, &iidEnumerator)
	if err != nil {
		return nil, err
	}
	defer release(en)
	var coll uintptr
	if err := vcall(en, 3, uintptr(flow), 1, uintptr(unsafe.Pointer(&coll))); err != nil {
		return nil, err
	}
	defer release(coll)
	var n uint32
	vcall(coll, 3, uintptr(unsafe.Pointer(&n)))
	var out []endpoint
	for i := range n {
		var dev uintptr
		if vcall(coll, 4, uintptr(i), uintptr(unsafe.Pointer(&dev))) != nil {
			continue
		}
		var idp, store uintptr
		var e endpoint
		if vcall(dev, 5, uintptr(unsafe.Pointer(&idp))) == nil {
			e.ID = takeString(idp)
		}
		if vcall(dev, 4, 0, uintptr(unsafe.Pointer(&store))) == nil {
			var pv [24]byte // PROPVARIANT: vt at 0, LPWSTR at 8
			if vcall(store, 5, uintptr(unsafe.Pointer(&pkeyFriendlyName)), uintptr(unsafe.Pointer(&pv))) == nil && pv[0] == 31 {
				e.Name = windows.UTF16PtrToString(*(**uint16)(unsafe.Pointer(&pv[8])))
			}
			pPropVariantClear.Call(uintptr(unsafe.Pointer(&pv)))
			release(store)
		}
		release(dev)
		out = append(out, e)
	}
	return out, nil
}

// defaultID returns the default endpoint ID for flow and role (0 console, 1 multimedia, 2 communications).
func defaultID(flow, role int) string {
	en, err := create(&clsidEnumerator, &iidEnumerator)
	if err != nil {
		return ""
	}
	defer release(en)
	var dev, idp uintptr
	if vcall(en, 4, uintptr(flow), uintptr(role), uintptr(unsafe.Pointer(&dev))) != nil {
		return ""
	}
	defer release(dev)
	if vcall(dev, 5, uintptr(unsafe.Pointer(&idp))) != nil {
		return ""
	}
	return takeString(idp)
}

func setDefault(id string, role int) error {
	pc, err := create(&clsidPolicyConfig, &iidPolicyConfig)
	if err != nil {
		return err
	}
	defer release(pc)
	p, _ := windows.UTF16PtrFromString(id)
	return vcall(pc, 13, uintptr(unsafe.Pointer(p)), uintptr(role))
}

var iidEndpointVolume = windows.GUID{Data1: 0x5CDF2C82, Data2: 0x841E, Data3: 0x4546, Data4: [8]byte{0x97, 0x22, 0x0C, 0xF7, 0x40, 0x78, 0x22, 0x9A}}

// withVolume opens IAudioEndpointVolume for an endpoint ID.
func withVolume(id string, f func(vol uintptr) error) error {
	en, err := create(&clsidEnumerator, &iidEnumerator)
	if err != nil {
		return err
	}
	defer release(en)
	p, _ := windows.UTF16PtrFromString(id)
	var dev, vol uintptr
	if err := vcall(en, 5, uintptr(unsafe.Pointer(p)), uintptr(unsafe.Pointer(&dev))); err != nil {
		return err
	}
	defer release(dev)
	if err := vcall(dev, 3, uintptr(unsafe.Pointer(&iidEndpointVolume)), 0x17, 0, uintptr(unsafe.Pointer(&vol))); err != nil {
		return err
	}
	defer release(vol)
	return f(vol)
}

func muted(id string) bool {
	var m int32
	withVolume(id, func(v uintptr) error { return vcall(v, 15, uintptr(unsafe.Pointer(&m))) })
	return m != 0
}

func setMuted(id string, on bool) error {
	b := uintptr(0)
	if on {
		b = 1
	}
	return withVolume(id, func(v uintptr) error { return vcall(v, 14, b, 0) })
}
