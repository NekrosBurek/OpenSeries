package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"slices"
	"strings"
	"sync"
	"time"

	"fyne.io/systray"
)

const appName = "OpenSeries"

type config struct {
	Out, In  endpoint // preferred devices; matched by ID, then by name if the USB port changed
	Unlocked bool     `json:",omitempty"` // false = keep forcing the preferred devices as default

	// Last values sent to the headset (it can't report them back). nil = never set here.
	Sidetone *int      `json:",omitempty"` // 0..3
	AutoOff  *int      `json:",omitempty"` // minutes, 0 = never
	EQ       []float32 `json:",omitempty"` // 10 bands, dB
	EQPreset string    `json:",omitempty"`
}

// snapshot is what the worker last saw; the UI renders it.
type snapshot struct {
	H         headset
	Status    string
	Outs, Ins []endpoint
	Cfg       config
	MicMuted  bool
	Note      string // result of the last device command
}

var (
	cfg     config
	cfgPath string
	jobs    = make(chan func(), 16) // everything touching cfg or COM runs on the worker
	snapMu  sync.Mutex
	snap    = snapshot{Status: "Starting…"}
)

func getSnap() snapshot {
	snapMu.Lock()
	defer snapMu.Unlock()
	return snap
}

func main() {
	if !singleInstance() {
		return // the running copy opens its window
	}
	dir, _ := os.UserConfigDir()
	dir = filepath.Join(dir, appName)
	os.MkdirAll(dir, 0o755)
	cfgPath = filepath.Join(dir, "config.json")
	if b, err := os.ReadFile(cfgPath); err == nil {
		json.Unmarshal(b, &cfg)
	}
	if f, err := os.Create(filepath.Join(dir, "openseries.log")); err == nil {
		log.SetOutput(f)
	}
	if exe, err := os.Executable(); err == nil {
		os.Remove(exe + ".old")
	}
	log.Println(appName, version)
	systray.Run(func() {
		systray.SetTitle(appName)
		systray.SetIcon(batteryIcon(headset{Dongle: true}))
		systray.SetOnTapped(showUI)
		status := systray.AddMenuItem(snap.Status, "")
		status.Disable()
		statusItem = status
		systray.AddSeparator()
		open := systray.AddMenuItem("Open "+appName, "")
		quit := systray.AddMenuItem("Quit", "")
		go func() {
			for {
				select {
				case <-open.ClickedCh:
					showUI()
				case <-quit.ClickedCh:
					systray.Quit()
				}
			}
		}()
		go waitShow(showUI)
		go worker()
		go func() {
			for time.Sleep(time.Minute); ; time.Sleep(24 * time.Hour) {
				if tag, err := selfUpdate(); err != nil {
					log.Println("update:", err)
				} else if tag != "" {
					log.Println("updated to", tag)
					restart()
				}
			}
		}()
		if !slices.Contains(os.Args, "--tray") {
			showUI()
		}
	}, nil)
}

func saveConfig() {
	b, _ := json.MarshalIndent(cfg, "", "  ")
	if err := os.WriteFile(cfgPath, b, 0o644); err != nil {
		log.Println("save config:", err)
	}
}

func restart() {
	exe, _ := os.Executable()
	releaseInstance()
	exec.Command(exe, "--tray").Start()
	systray.Quit()
}

var statusItem *systray.MenuItem

// ponytail: polls every 2s; switch to IMMNotificationClient callbacks if instant revert ever matters.
func worker() {
	runtime.LockOSThread()
	comInit()
	var h headset
	var herr error
	var lastPoll time.Time
	var lastRaw string
	lowWarned := false
	tick := time.NewTicker(2 * time.Second)
	for {
		outs, _ := endpoints(flowOut)
		ins, _ := endpoints(flowIn)
		if cfg.Out.ID == "" || cfg.In.ID == "" {
			autoPick(outs, &cfg.Out)
			autoPick(ins, &cfg.In)
		}
		if !cfg.Unlocked {
			enforce(flowOut, outs, cfg.Out)
			enforce(flowIn, ins, cfg.In)
		}
		// Poll fast while off so switching the headset on shows up quickly.
		if every := map[bool]time.Duration{true: 10 * time.Second, false: 3 * time.Second}[h.Online]; time.Since(lastPoll) >= every {
			lastPoll = time.Now()
			prev := h
			h, herr = readHeadset()
			if prev.Online != h.Online || prev.Dongle != h.Dongle || herr != nil {
				log.Printf("headset: %+v err=%v", h, herr)
			}
			if raw := fmt.Sprintf("% x", h.Raw[:min(len(h.Raw), 16)]); raw != lastRaw {
				lastRaw = raw
				log.Println("raw:", raw) // for mapping undocumented bytes (e.g. mic mute button)
			}
			systray.SetIcon(batteryIcon(h))
			if h.Online && !h.Charging && h.Battery == 0 && !lowWarned {
				lowWarned = true
				go ask("Headset battery is almost empty.", false)
			}
			lowWarned = lowWarned && !h.Charging && h.Battery == 0
		}
		mic, _ := resolve(ins, cfg.In)
		publish(snapshot{H: h, Status: statusText(h, herr), Outs: outs, Ins: ins, Cfg: cfg, MicMuted: mic.ID != "" && muted(mic.ID), Note: note})
		select {
		case f := <-jobs:
			f()
		case <-tick.C:
		}
	}
}

var note string // worker-owned

// device runs a headset command on the worker and records the outcome for the UI.
func device(what string, send func() error, save func()) {
	jobs <- func() {
		if err := send(); err != nil {
			note = what + " failed: " + err.Error()
			log.Println(note)
			return
		}
		note = ""
		save()
		saveConfig()
	}
}

func publish(s snapshot) {
	s.H.Raw = nil
	snapMu.Lock()
	changed := fmt.Sprint(s) != fmt.Sprint(snap)
	snap = s
	snapMu.Unlock()
	if changed {
		systray.SetTooltip(s.Status)
		statusItem.SetTitle(s.Status)
		invalidateUI()
	}
}

func statusText(h headset, err error) string {
	switch {
	case !h.Dongle:
		return "Receiver not connected"
	case err != nil:
		return "Receiver not responding"
	case !h.Online:
		return "Headset off / out of range"
	case h.Charging:
		return fmt.Sprintf("Connected · %d%% · charging", h.Battery)
	default:
		return fmt.Sprintf("Connected · %d%%", h.Battery)
	}
}

var portPrefix = regexp.MustCompile(`\d+\s*-\s*`) // "Headphones (4- Arctis 7+)" → "Headphones (Arctis 7+)"

func resolve(eps []endpoint, ref endpoint) (endpoint, bool) {
	for _, e := range eps {
		if e.ID == ref.ID {
			return e, true
		}
	}
	for _, e := range eps {
		if ref.Name != "" && portPrefix.ReplaceAllString(e.Name, "") == portPrefix.ReplaceAllString(ref.Name, "") {
			return e, true
		}
	}
	return endpoint{}, false
}

func autoPick(eps []endpoint, ref *endpoint) {
	if ref.ID != "" {
		return
	}
	for _, e := range eps {
		if strings.Contains(e.Name, "Arctis") {
			*ref = e
			log.Println("auto-picked", e.Name)
			saveConfig()
			return
		}
	}
}

var roleNames = []string{"default", "multimedia", "communications"}

func enforce(flow int, eps []endpoint, ref endpoint) {
	want, ok := resolve(eps, ref)
	if !ok {
		return // preferred device unplugged: leave Windows alone
	}
	for role := range 3 {
		if cur := defaultID(flow, role); cur != want.ID {
			err := setDefault(want.ID, role)
			log.Printf("%s default was %q, set %q: %v", roleNames[role], nameOf(eps, cur), want.Name, err)
		}
	}
}

func nameOf(eps []endpoint, id string) string {
	for _, e := range eps {
		if e.ID == id {
			return e.Name
		}
	}
	return id
}

type diagLine struct {
	OK   bool
	Text string
}

// diagnose must run on the worker. Returns the report, plus the receiver's USB
// instance ID when restarting it may help.
func diagnose() ([]diagLine, string) {
	var out []diagLine
	line := func(ok bool, f string, a ...any) { out = append(out, diagLine{ok, fmt.Sprintf(f, a...)}) }
	hwBad := false
	usb := ""
	devs := pnpDevices()
	for _, d := range devs {
		if up := strings.ToUpper(d.ID); strings.HasPrefix(up, `USB\`) && !strings.Contains(up, "&MI_") {
			usb = d.ID
		}
		if d.Problem != 0 {
			hwBad = true
			line(false, "Driver problem: %s (Device Manager code %d)", d.Name, d.Problem)
		}
	}
	h, err := readHeadset()
	switch {
	case len(devs) == 0 || !h.Dongle:
		line(false, "USB receiver not found: plug it in, avoid hubs/front ports")
	case err != nil:
		hwBad = true
		line(false, "Receiver not responding: %v", err)
	case !h.Online:
		line(false, "Headset off or out of range (receiver OK)")
	default:
		line(true, "Headset connected, battery %d%%%s", h.Battery, map[bool]string{true: ", charging"}[h.Charging])
	}
	if len(devs) > 0 && !hwBad {
		line(true, "Drivers OK (%d device nodes)", len(devs))
	}

	procs := steelseriesProcs()
	sonar := false
	mb := 0
	for _, p := range procs {
		sonar = sonar || strings.Contains(strings.ToLower(p.Name), "sonar")
		mb += max(p.MB, 0)
	}
	for _, f := range []struct {
		flow  int
		label string
		ref   endpoint
	}{{flowOut, "Output", cfg.Out}, {flowIn, "Input", cfg.In}} {
		eps, _ := endpoints(f.flow)
		want, ok := resolve(eps, f.ref)
		if !ok {
			line(false, "%s device not found: %s", f.label, f.ref.Name)
		}
		for _, role := range []int{0, 2} {
			cur := nameOf(eps, defaultID(f.flow, role))
			line(!ok || cur == want.Name, "Windows %s %s: %s", roleNames[role], strings.ToLower(f.label), cur)
			if strings.Contains(cur, "Sonar") && !sonar {
				line(false, "That's a Sonar device but Sonar isn't running, so it's silent")
			}
		}
	}
	if len(procs) == 0 {
		line(true, "SteelSeries GG not running")
	} else {
		line(true, "SteelSeries GG: %d processes, %d MB RAM (Sonar %s)", len(procs), mb, map[bool]string{true: "running", false: "not running"}[sonar])
	}
	if cfg.Unlocked {
		line(false, "Default-device lock is off")
	}
	if !hwBad {
		usb = ""
	}
	return out, usb
}
