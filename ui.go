package main

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"strings"
	"sync"

	"gioui.org/app"
	"gioui.org/f32"
	"gioui.org/font/gofont"
	"gioui.org/io/system"
	"gioui.org/layout"
	"gioui.org/op"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/text"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"
)

// The window exists only while open; closing it frees the UI, the tray keeps running.
var (
	uiMu  sync.Mutex
	uiWin *app.Window
)

func showUI() {
	uiMu.Lock()
	defer uiMu.Unlock()
	if uiWin != nil {
		uiWin.Perform(system.ActionRaise)
		return
	}
	uiWin = new(app.Window)
	go runUI(uiWin)
}

func invalidateUI() {
	uiMu.Lock()
	if uiWin != nil {
		uiWin.Invalidate()
	}
	uiMu.Unlock()
}

var (
	colBg     = color.NRGBA{0x14, 0x16, 0x1b, 0xff}
	colCard   = color.NRGBA{0x1f, 0x22, 0x29, 0xff}
	colRaised = color.NRGBA{0x2c, 0x30, 0x39, 0xff}
	colFg     = color.NRGBA{0xe8, 0xe8, 0xea, 0xff}
	colMuted  = color.NRGBA{0x8b, 0x90, 0x9a, 0xff}
	colAccent = color.NRGBA{0xff, 0x5a, 0x1f, 0xff}
	colOK     = color.NRGBA{0x3c, 0xc8, 0x5a, 0xff}
	colBad    = color.NRGBA{0xe6, 0x4b, 0x4b, 0xff}
	colCharge = color.NRGBA{0x3c, 0x96, 0xf0, 0xff}
)

type D = layout.Dimensions
type C = layout.Context

var (
	eqBands   = []string{"31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"}
	eqPresets = []struct {
		Name  string
		Bands []float32
	}{
		{"Flat", []float32{0, 0, 0, 0, 0, 0, 0, 0, 0, 0}},
		{"Bass Boost", []float32{3.5, 4, 1, -1.5, -1.5, -1, -1, -1, -1, 5.5}},
		{"Smiley", []float32{3, 1.5, -1.5, -4, -4, -2.5, 1.5, 3, 4, 3.5}},
		{"Focus", []float32{-5, -1, -3.5, -2.5, 4, 6, 3.5, -3.5, 0, -3.5}},
	}
	sidetoneNames = []string{"Off", "Low", "Medium", "High"}
	autoOffMins   = []int{0, 10, 30, 60, 90}
)

type ui struct {
	th                              *material.Theme
	w                               *app.Window
	tab                             int
	tabs                            [3]widget.Clickable
	out, in                         widget.Enum
	lock, auto                      widget.Bool
	diagBtn, fixBtn, updBtn, micBtn widget.Clickable
	side                            [4]widget.Clickable
	off                             [5]widget.Clickable
	presets                         [4]widget.Clickable
	eq                              [10]widget.Float
	eqDirty, eqInit                 bool
	list                            widget.List
	report                          []diagLine
	usb, msg                        string
	busy                            bool
	results                         chan func() // background results, applied on the UI goroutine
}

func runUI(w *app.Window) {
	w.Option(app.Title(appName), app.Size(unit.Dp(500), unit.Dp(780)), app.MinSize(unit.Dp(420), unit.Dp(500)))
	th := material.NewTheme()
	th.Shaper = text.NewShaper(text.WithCollection(gofont.Collection()))
	th.Palette = material.Palette{Bg: colBg, Fg: colFg, ContrastBg: colAccent, ContrastFg: colFg}
	u := &ui{th: th, w: w, results: make(chan func(), 4)}
	u.list.Axis = layout.Vertical
	u.auto.Value = autostart()
	var ops op.Ops
	for {
		switch e := w.Event().(type) {
		case app.DestroyEvent:
			uiMu.Lock()
			uiWin = nil
			uiMu.Unlock()
			return
		case app.FrameEvent:
			gtx := app.NewContext(&ops, e)
			u.frame(gtx)
			e.Frame(gtx.Ops)
		}
	}
}

func (u *ui) background(f func() func()) {
	u.busy = true
	go func() { u.results <- f(); u.w.Invalidate() }()
}

func (u *ui) frame(gtx C) {
drain:
	for {
		select {
		case f := <-u.results:
			f()
			u.busy = false
		default:
			break drain
		}
	}
	s := getSnap()
	u.handle(gtx, s)

	paint.Fill(gtx.Ops, colBg)
	var sections []layout.Widget
	sections = append(sections, func(gtx C) D { return u.header(gtx, s) }, u.tabBar)
	switch u.tab {
	case 0:
		sections = append(sections, u.deviceTab(s)...)
	case 1:
		sections = append(sections, u.audioTab(s)...)
	case 2:
		sections = append(sections, u.troubleTab())
	}
	sections = append(sections, u.footer)
	material.List(u.th, &u.list).Layout(gtx, len(sections), func(gtx C, i int) D { return sections[i](gtx) })
}

// handle applies input. User changes go to the worker; otherwise widgets mirror worker state.
func (u *ui) handle(gtx C, s snapshot) {
	for i := range u.tabs {
		if u.tabs[i].Clicked(gtx) {
			u.tab = i
		}
	}
	if u.out.Update(gtx) {
		pick(s.Outs, u.out.Value, &cfg.Out)
	} else {
		cur, _ := resolve(s.Outs, s.Cfg.Out)
		u.out.Value = cur.ID
	}
	if u.in.Update(gtx) {
		pick(s.Ins, u.in.Value, &cfg.In)
	} else {
		cur, _ := resolve(s.Ins, s.Cfg.In)
		u.in.Value = cur.ID
	}
	if u.lock.Update(gtx) {
		v := u.lock.Value
		jobs <- func() { cfg.Unlocked = !v; saveConfig() }
	} else {
		u.lock.Value = !s.Cfg.Unlocked
	}
	if u.auto.Update(gtx) {
		if err := setAutostart(u.auto.Value); err != nil {
			u.msg = "Autostart: " + err.Error()
		}
		u.auto.Value = autostart()
	}
	if u.micBtn.Clicked(gtx) {
		m := !s.MicMuted
		jobs <- func() {
			if mic, ok := resolve(getSnap().Ins, cfg.In); ok {
				setMuted(mic.ID, m)
			}
		}
	}
	for i := range u.side {
		if u.side[i].Clicked(gtx) {
			device("Sidetone", func() error { return setSidetone(i) }, func() { cfg.Sidetone = &i })
		}
	}
	for i, m := range autoOffMins {
		if u.off[i].Clicked(gtx) {
			device("Auto-off", func() error { return setAutoOff(m) }, func() { cfg.AutoOff = &m })
		}
	}

	// EQ: sliders are local while dragging; sent to the headset on release.
	if !u.eqInit {
		u.eqInit = true
		for i := range u.eq {
			u.eq[i].Value = 0.5
			if len(s.Cfg.EQ) == 10 {
				u.eq[i].Value = (s.Cfg.EQ[i] + 12) / 24
			}
		}
	}
	for i := range u.presets {
		if u.presets[i].Clicked(gtx) {
			p := eqPresets[i]
			for j := range u.eq {
				u.eq[j].Value = (p.Bands[j] + 12) / 24
			}
			u.sendEQ(p.Name)
		}
	}
	dragging := false
	for i := range u.eq {
		if u.eq[i].Update(gtx) {
			u.eqDirty = true
		}
		dragging = dragging || u.eq[i].Dragging()
	}
	if u.eqDirty && !dragging {
		u.sendEQ("Custom")
	}

	if u.diagBtn.Clicked(gtx) && !u.busy {
		u.msg = ""
		u.background(func() func() {
			done := make(chan struct{})
			var r []diagLine
			var usb string
			jobs <- func() { r, usb = diagnose(); close(done) }
			<-done
			return func() { u.report, u.usb = r, usb }
		})
	}
	if u.fixBtn.Clicked(gtx) && u.usb != "" {
		if err := restartDongle(u.usb); err != nil {
			u.msg = "Restart failed: " + err.Error()
		} else {
			u.msg = "Receiver restarting… run the checks again in a few seconds."
			u.usb = ""
		}
	}
	if u.updBtn.Clicked(gtx) && !u.busy {
		u.msg = "Checking for updates…"
		u.background(func() func() {
			tag, err := selfUpdate()
			return func() {
				switch {
				case err != nil:
					u.msg = "Update failed: " + err.Error()
				case tag != "":
					restart()
				case version == "dev":
					u.msg = "Development build: updates are off."
				default:
					u.msg = "You're on the latest version."
				}
			}
		})
	}
}

func (u *ui) sendEQ(name string) {
	u.eqDirty = false
	var bands [10]float32
	for i := range u.eq {
		bands[i] = float32(math.Round(float64(u.eq[i].Value*24-12)*2) / 2)
	}
	device("Equalizer", func() error { return setEQ(bands) }, func() { cfg.EQ = bands[:]; cfg.EQPreset = name })
}

func pick(eps []endpoint, id string, ref *endpoint) {
	for _, e := range eps {
		if e.ID == id {
			jobs <- func() { *ref = e; saveConfig() }
		}
	}
}

func batteryColor(h headset) color.NRGBA {
	switch {
	case !h.Dongle:
		return colBad
	case !h.Online:
		return colMuted
	case h.Charging:
		return colCharge
	case h.Battery <= 25:
		return colBad
	}
	return colOK
}

func (u *ui) header(gtx C, s snapshot) D {
	pct := "—"
	if s.H.Online {
		pct = fmt.Sprintf("%d%%", s.H.Battery)
	}
	return layout.Inset{Left: 20, Right: 20, Top: 18, Bottom: 6}.Layout(gtx, func(gtx C) D {
		return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
			layout.Flexed(1, func(gtx C) D {
				return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
					layout.Rigid(material.H5(u.th, "Arctis 7+").Layout),
					layout.Rigid(label(u.th, 14, s.Status, batteryColor(s.H))),
				)
			}),
			layout.Rigid(label(u.th, 40, pct, batteryColor(s.H))),
		)
	})
}

func (u *ui) tabBar(gtx C) D {
	names := []string{"Device", "Audio", "Troubleshoot"}
	children := make([]layout.FlexChild, len(names))
	for i, n := range names {
		children[i] = layout.Flexed(1, func(gtx C) D { return u.segment(gtx, &u.tabs[i], n, u.tab == i) })
	}
	return layout.Inset{Left: 12, Right: 12, Top: 6, Bottom: 6}.Layout(gtx, func(gtx C) D {
		return layout.Flex{}.Layout(gtx, children...)
	})
}

// segment is a flat toggle-style button.
func (u *ui) segment(gtx C, c *widget.Clickable, txt string, on bool) D {
	return layout.UniformInset(3).Layout(gtx, func(gtx C) D {
		b := material.Button(u.th, c, txt)
		b.Background, b.Color, b.CornerRadius = colRaised, colFg, 8
		b.Inset = layout.Inset{Top: 9, Bottom: 9, Left: 6, Right: 6}
		b.TextSize = 14
		if on {
			b.Background = colAccent
		}
		gtx.Constraints.Min.X = gtx.Constraints.Max.X
		return b.Layout(gtx)
	})
}

func (u *ui) deviceTab(s snapshot) []layout.Widget {
	ws := []layout.Widget{
		func(gtx C) D {
			return card(gtx, u.th, "", func(gtx C) D {
				return layout.Flex{Axis: layout.Vertical, Alignment: layout.Middle}.Layout(gtx,
					layout.Rigid(func(gtx C) D { return headsetArt(gtx, u.th, s) }),
					layout.Rigid(layout.Spacer{Height: 8}.Layout),
					layout.Rigid(func(gtx C) D {
						txt, col := "Mic live · click to mute", colOK
						if s.MicMuted {
							txt, col = "Mic muted · click to unmute", colBad
						}
						b := material.Button(u.th, &u.micBtn, txt)
						b.Background, b.Color, b.TextSize = colRaised, col, 14
						return b.Layout(gtx)
					}),
				)
			})
		},
		func(gtx C) D {
			return card(gtx, u.th, "ChatMix", func(gtx C) D {
				bar := func(name string, v int, col color.NRGBA) layout.FlexChild {
					return layout.Rigid(func(gtx C) D {
						return layout.Inset{Bottom: 6}.Layout(gtx, func(gtx C) D {
							return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
								layout.Rigid(func(gtx C) D {
									gtx.Constraints.Min.X = gtx.Dp(56)
									return material.Body2(u.th, name).Layout(gtx)
								}),
								layout.Flexed(1, func(gtx C) D {
									p := material.ProgressBar(u.th, float32(v)/100)
									p.Color, p.TrackColor, p.Height = col, colRaised, 8
									return p.Layout(gtx)
								}),
								layout.Rigid(func(gtx C) D {
									gtx.Constraints.Min.X = gtx.Dp(48)
									return layout.E.Layout(gtx, label(u.th, 14, fmt.Sprintf("%d%%", v), colMuted))
								}),
							)
						})
					})
				}
				return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
					bar("Game", s.H.Game, colAccent),
					bar("Chat", s.H.Chat, colCharge),
					layout.Rigid(label(u.th, 12, "Turn the dial on the right earcup. Chat goes to the Chat device in Windows.", colMuted)),
				)
			})
		},
		func(gtx C) D {
			return card(gtx, u.th, "Sidetone (hear your own voice)", func(gtx C) D {
				return u.segments(gtx, u.side[:], sidetoneNames, s.Cfg.Sidetone)
			})
		},
		func(gtx C) D { return card(gtx, u.th, "Equalizer (stored on headset)", u.eqView(s)) },
		func(gtx C) D {
			return card(gtx, u.th, "Auto power-off", func(gtx C) D {
				names := []string{"Never", "10 min", "30 min", "1 h", "90 min"}
				var cur *int
				if s.Cfg.AutoOff != nil {
					for i, m := range autoOffMins {
						if m == *s.Cfg.AutoOff {
							cur = &i
						}
					}
				}
				return u.segments(gtx, u.off[:], names, cur)
			})
		},
	}
	if s.Note != "" {
		ws = append(ws, func(gtx C) D {
			return layout.Inset{Left: 20, Right: 20, Top: 4}.Layout(gtx, label(u.th, 13, s.Note, colBad))
		})
	}
	return ws
}

func (u *ui) segments(gtx C, cs []widget.Clickable, names []string, cur *int) D {
	children := make([]layout.FlexChild, len(names))
	for i, n := range names {
		children[i] = layout.Flexed(1, func(gtx C) D { return u.segment(gtx, &cs[i], n, cur != nil && *cur == i) })
	}
	return layout.Flex{}.Layout(gtx, children...)
}

func (u *ui) eqView(s snapshot) layout.Widget {
	return func(gtx C) D {
		presets := make([]string, len(eqPresets))
		for i, p := range eqPresets {
			presets[i] = p.Name
		}
		var cur *int
		for i, p := range eqPresets {
			if p.Name == s.Cfg.EQPreset {
				cur = &i
			}
		}
		sliders := make([]layout.FlexChild, len(u.eq))
		for i := range u.eq {
			sliders[i] = layout.Flexed(1, func(gtx C) D {
				db := math.Round(float64(u.eq[i].Value*24-12)*2) / 2
				return layout.Flex{Axis: layout.Vertical, Alignment: layout.Middle}.Layout(gtx,
					layout.Rigid(label(u.th, 11, fmt.Sprintf("%+.1f", db), colMuted)),
					layout.Rigid(func(gtx C) D {
						gtx.Constraints.Min.Y = gtx.Dp(140)
						gtx.Constraints.Max.Y = gtx.Dp(140)
						sl := material.Slider(u.th, &u.eq[i])
						sl.Axis = layout.Vertical
						return sl.Layout(gtx)
					}),
					layout.Rigid(label(u.th, 11, eqBands[i], colMuted)),
				)
			})
		}
		return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
			layout.Rigid(func(gtx C) D { return u.segments(gtx, u.presets[:], presets, cur) }),
			layout.Rigid(layout.Spacer{Height: 10}.Layout),
			layout.Rigid(func(gtx C) D { return layout.Flex{}.Layout(gtx, sliders...) }),
		)
	}
}

func (u *ui) audioTab(s snapshot) []layout.Widget {
	return []layout.Widget{
		func(gtx C) D {
			return card(gtx, u.th, "Headphones / speakers", func(gtx C) D { return radios(gtx, u.th, &u.out, s.Outs) })
		},
		func(gtx C) D {
			return card(gtx, u.th, "Microphone", func(gtx C) D { return radios(gtx, u.th, &u.in, s.Ins) })
		},
		func(gtx C) D {
			return card(gtx, u.th, "Behaviour", func(gtx C) D {
				return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
					layout.Rigid(func(gtx C) D {
						return toggle(gtx, u.th, &u.lock, "Always use these as default", "Calls and meetings included. Reverts GG or apps changing it.")
					}),
					layout.Rigid(layout.Spacer{Height: 12}.Layout),
					layout.Rigid(func(gtx C) D { return toggle(gtx, u.th, &u.auto, "Start with Windows", "Starts quietly in the tray.") }),
				)
			})
		},
	}
}

func (u *ui) troubleTab() layout.Widget {
	return func(gtx C) D {
		return card(gtx, u.th, "Troubleshoot", func(gtx C) D {
			rows := []layout.FlexChild{layout.Rigid(func(gtx C) D {
				return layout.Flex{}.Layout(gtx,
					layout.Rigid(material.Button(u.th, &u.diagBtn, map[bool]string{true: "Working…", false: "Run checks"}[u.busy]).Layout),
					layout.Rigid(layout.Spacer{Width: 8}.Layout),
					layout.Rigid(func(gtx C) D {
						if u.usb == "" {
							return D{}
						}
						b := material.Button(u.th, &u.fixBtn, "Restart receiver")
						b.Background = colBad
						return b.Layout(gtx)
					}),
				)
			})}
			for _, l := range u.report {
				rows = append(rows, layout.Rigid(func(gtx C) D {
					return layout.Inset{Top: 8}.Layout(gtx, func(gtx C) D { return diagRow(gtx, u.th, l) })
				}))
			}
			return layout.Flex{Axis: layout.Vertical}.Layout(gtx, rows...)
		})
	}
}

func (u *ui) footer(gtx C) D {
	return layout.Inset{Left: 20, Right: 20, Top: 4, Bottom: 20}.Layout(gtx, func(gtx C) D {
		return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
			layout.Flexed(1, label(u.th, 12, appName+" "+version+map[bool]string{true: "\n" + u.msg}[u.msg != ""], colMuted)),
			layout.Rigid(func(gtx C) D {
				b := material.Button(u.th, &u.updBtn, "Check for updates")
				b.Background, b.TextSize = colCard, 14
				return b.Layout(gtx)
			}),
		)
	})
}

// headsetArt draws a front view of the headset with live indicators:
// battery in the left cup, ChatMix dial on the right cup, mic LED on the boom tip.
func headsetArt(gtx C, th *material.Theme, s snapshot) D {
	px := gtx.Metric.PxPerDp
	w, h := gtx.Constraints.Max.X, int(230*px)
	cx := float32(w) / 2
	pt := func(x, y float32) f32.Point { return f32.Pt(cx+x*px, y*px) }
	stroke := func(build func(p *clip.Path), width float32, col color.NRGBA) {
		var p clip.Path
		p.Begin(gtx.Ops)
		build(&p)
		paint.FillShape(gtx.Ops, col, clip.Stroke{Path: p.End(), Width: width * px}.Op())
	}
	rrect := func(x, y, rw, rh, r float32, col color.NRGBA) {
		rect := image.Rect(int(cx+x*px), int(y*px), int(cx+(x+rw)*px), int((y+rh)*px))
		paint.FillShape(gtx.Ops, col, clip.UniformRRect(rect, int(r*px)).Op(gtx.Ops))
	}
	circle := func(x, y, r float32, col color.NRGBA) {
		rect := image.Rect(int(cx+(x-r)*px), int((y-r)*px), int(cx+(x+r)*px), int((y+r)*px))
		paint.FillShape(gtx.Ops, col, clip.Ellipse(rect).Op(gtx.Ops))
	}

	// Headband: half circle over the top, with an accent stripe.
	band := func(p *clip.Path) { p.MoveTo(pt(-92, 110)); p.Arc(pt(92, 110).Sub(pt(-92, 110)).Mul(0.5), pt(92, 110).Sub(pt(-92, 110)).Mul(0.5), math.Pi) }
	stroke(band, 16, colRaised)
	stroke(band, 3, colAccent)
	// Yokes.
	rrect(-100, 92, 16, 30, 4, colRaised)
	rrect(84, 92, 16, 30, 4, colRaised)
	// Mic boom from the left cup, curving towards the mouth, LED at the tip.
	stroke(func(p *clip.Path) { p.MoveTo(pt(-100, 175)); p.QuadTo(pt(-104, 222), pt(-42, 214)) }, 6, colRaised)
	micCol := colOK
	if s.MicMuted {
		micCol = colBad
	}
	circle(-40, 214, 7, micCol)
	// Ear cups.
	for _, x := range []float32{-130, 70} {
		rrect(x, 112, 60, 100, 26, colRaised)
		rrect(x+8, 120, 44, 84, 20, color.NRGBA{0x24, 0x27, 0x2f, 0xff})
	}
	// Left cup: battery bars.
	bc := batteryColor(s.H)
	rrect(-112, 140, 24, 44, 4, colMuted)
	rrect(-107, 136, 14, 5, 2, colMuted)
	rrect(-110, 142, 20, 40, 3, color.NRGBA{0x18, 0x1a, 0x1f, 0xff})
	if s.H.Online {
		for i := range s.H.Battery / 25 {
			rrect(-108, 172-float32(i)*10, 16, 8, 2, bc)
		}
	}
	// Right cup: ChatMix dial; needle leans to Game (left) or Chat (right).
	circle(100, 162, 18, colMuted)
	circle(100, 162, 15, color.NRGBA{0x18, 0x1a, 0x1f, 0xff})
	a := float64(s.H.Chat-s.H.Game) / 100 * (math.Pi / 2.2)
	stroke(func(p *clip.Path) {
		p.MoveTo(pt(100, 162))
		p.LineTo(pt(100+13*float32(math.Sin(a)), 162-13*float32(math.Cos(a))))
	}, 3, colAccent)

	// Captions under the cups.
	caption := func(x float32, txt string, col color.NRGBA) {
		defer op.Offset(image.Pt(int(cx+x*px), int(190*px))).Push(gtx.Ops).Pop()
		g := gtx
		g.Constraints = layout.Exact(image.Pt(int(60*px), int(20*px)))
		layout.Center.Layout(g, label(th, 11, txt, col))
	}
	if s.H.Online {
		caption(-130, fmt.Sprintf("%d%%", s.H.Battery), bc)
	}
	caption(70, "ChatMix", colMuted)
	return D{Size: image.Pt(w, h)}
}

func label(th *material.Theme, size float32, txt string, col color.NRGBA) layout.Widget {
	l := material.Label(th, unit.Sp(size), txt)
	l.Color = col
	return l.Layout
}

func card(gtx C, th *material.Theme, title string, body layout.Widget) D {
	return layout.Inset{Left: 12, Right: 12, Top: 6, Bottom: 6}.Layout(gtx, func(gtx C) D {
		return layout.Background{}.Layout(gtx,
			func(gtx C) D {
				defer clip.UniformRRect(image.Rectangle{Max: gtx.Constraints.Min}, gtx.Dp(10)).Push(gtx.Ops).Pop()
				paint.Fill(gtx.Ops, colCard)
				return D{Size: gtx.Constraints.Min}
			},
			func(gtx C) D {
				return layout.UniformInset(16).Layout(gtx, func(gtx C) D {
					gtx.Constraints.Min.X = gtx.Constraints.Max.X
					if title == "" {
						return body(gtx)
					}
					return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
						layout.Rigid(label(th, 13, strings.ToUpper(title), colMuted)),
						layout.Rigid(layout.Spacer{Height: 8}.Layout),
						layout.Rigid(body),
					)
				})
			})
	})
}

func radios(gtx C, th *material.Theme, group *widget.Enum, eps []endpoint) D {
	if len(eps) == 0 {
		return material.Body2(th, "No active devices").Layout(gtx)
	}
	rows := make([]layout.FlexChild, len(eps))
	for i, e := range eps {
		rows[i] = layout.Rigid(func(gtx C) D {
			name := strings.Replace(e.Name, " (SteelSeries Sonar Virtual Audio Device)", "", 1)
			return material.RadioButton(th, group, e.ID, name).Layout(gtx)
		})
	}
	return layout.Flex{Axis: layout.Vertical}.Layout(gtx, rows...)
}

func toggle(gtx C, th *material.Theme, b *widget.Bool, title, sub string) D {
	return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
		layout.Flexed(1, func(gtx C) D {
			return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
				layout.Rigid(material.Body1(th, title).Layout),
				layout.Rigid(label(th, 12, sub, colMuted)),
			)
		}),
		layout.Rigid(material.Switch(th, b, title).Layout),
	)
}

func diagRow(gtx C, th *material.Theme, l diagLine) D {
	col := colOK
	if !l.OK {
		col = colBad
	}
	return layout.Flex{}.Layout(gtx,
		layout.Rigid(func(gtx C) D {
			return layout.Inset{Top: 6, Right: 10}.Layout(gtx, func(gtx C) D {
				sz := gtx.Dp(8)
				defer clip.Ellipse{Max: image.Pt(sz, sz)}.Push(gtx.Ops).Pop()
				paint.Fill(gtx.Ops, col)
				return D{Size: image.Pt(sz, sz)}
			})
		}),
		layout.Flexed(1, material.Body2(th, l.Text).Layout),
	)
}
