package main

import (
	"image"
	"image/color"
	"strconv"
	"strings"
	"sync"

	"gioui.org/app"
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
	colFg     = color.NRGBA{0xe8, 0xe8, 0xea, 0xff}
	colMuted  = color.NRGBA{0x8b, 0x90, 0x9a, 0xff}
	colAccent = color.NRGBA{0xff, 0x5a, 0x1f, 0xff}
	colOK     = color.NRGBA{0x3c, 0xc8, 0x5a, 0xff}
	colBad    = color.NRGBA{0xe6, 0x4b, 0x4b, 0xff}
	colCharge = color.NRGBA{0x3c, 0x96, 0xf0, 0xff}
)

type D = layout.Dimensions
type C = layout.Context

func runUI(w *app.Window) {
	w.Option(app.Title(appName), app.Size(unit.Dp(480), unit.Dp(720)), app.MinSize(unit.Dp(380), unit.Dp(400)))
	th := material.NewTheme()
	th.Shaper = text.NewShaper(text.WithCollection(gofont.Collection()))
	th.Palette = material.Palette{Bg: colBg, Fg: colFg, ContrastBg: colAccent, ContrastFg: colFg}

	var (
		ops                     op.Ops
		out, in                 widget.Enum
		lock, auto              widget.Bool
		diagBtn, fixBtn, updBtn widget.Clickable
		list                    = widget.List{List: layout.List{Axis: layout.Vertical}}
		report                  []diagLine
		usb, msg                string
		busy                    bool
		results                 = make(chan func(), 4) // background results, applied on this goroutine
	)
	auto.Value = autostart()
	background := func(f func() func()) {
		busy = true
		go func() { results <- f(); w.Invalidate() }()
	}

	for {
		switch e := w.Event().(type) {
		case app.DestroyEvent:
			uiMu.Lock()
			uiWin = nil
			uiMu.Unlock()
			return
		case app.FrameEvent:
			gtx := app.NewContext(&ops, e)
		drain:
			for {
				select {
				case f := <-results:
					f()
					busy = false
				default:
					break drain
				}
			}
			s := getSnap()

			// Input handling: user changes go to the worker; otherwise mirror worker state.
			if out.Update(gtx) {
				pick(s.Outs, out.Value, &cfg.Out)
			} else {
				cur, _ := resolve(s.Outs, s.Cfg.Out)
				out.Value = cur.ID
			}
			if in.Update(gtx) {
				pick(s.Ins, in.Value, &cfg.In)
			} else {
				cur, _ := resolve(s.Ins, s.Cfg.In)
				in.Value = cur.ID
			}
			if lock.Update(gtx) {
				v := lock.Value
				jobs <- func() { cfg.Unlocked = !v; saveConfig() }
			} else {
				lock.Value = !s.Cfg.Unlocked
			}
			if auto.Update(gtx) {
				if err := setAutostart(auto.Value); err != nil {
					msg = "Autostart: " + err.Error()
				}
				auto.Value = autostart()
			}
			if diagBtn.Clicked(gtx) && !busy {
				msg = ""
				background(func() func() {
					done := make(chan struct{})
					var r []diagLine
					var u string
					jobs <- func() { r, u = diagnose(); close(done) }
					<-done
					return func() { report, usb = r, u }
				})
			}
			if fixBtn.Clicked(gtx) && usb != "" {
				if err := restartDongle(usb); err != nil {
					msg = "Restart failed: " + err.Error()
				} else {
					msg = "Receiver restarting… run Troubleshoot again in a few seconds."
					usb = ""
				}
			}
			if updBtn.Clicked(gtx) && !busy {
				msg = "Checking for updates…"
				background(func() func() {
					tag, err := selfUpdate()
					return func() {
						switch {
						case err != nil:
							msg = "Update failed: " + err.Error()
						case tag != "":
							restart()
						case version == "dev":
							msg = "Development build: updates are off."
						default:
							msg = "You're on the latest version."
						}
					}
				})
			}

			paint.Fill(gtx.Ops, colBg)
			sections := []layout.Widget{
				func(gtx C) D { return header(gtx, th, s) },
				func(gtx C) D {
					return card(gtx, th, "Headphones / speakers", func(gtx C) D { return radios(gtx, th, &out, s.Outs) })
				},
				func(gtx C) D {
					return card(gtx, th, "Microphone", func(gtx C) D { return radios(gtx, th, &in, s.Ins) })
				},
				func(gtx C) D {
					return card(gtx, th, "Behaviour", func(gtx C) D {
						return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
							layout.Rigid(func(gtx C) D {
								return toggle(gtx, th, &lock, "Always use these as default", "Calls and meetings included. Reverts GG or apps changing it.")
							}),
							layout.Rigid(layout.Spacer{Height: 12}.Layout),
							layout.Rigid(func(gtx C) D { return toggle(gtx, th, &auto, "Start with Windows", "Starts quietly in the tray.") }),
						)
					})
				},
				func(gtx C) D {
					return card(gtx, th, "Troubleshoot", func(gtx C) D {
						rows := []layout.FlexChild{layout.Rigid(func(gtx C) D {
							return layout.Flex{}.Layout(gtx,
								layout.Rigid(material.Button(th, &diagBtn, map[bool]string{true: "Working…", false: "Run checks"}[busy]).Layout),
								layout.Rigid(layout.Spacer{Width: 8}.Layout),
								layout.Rigid(func(gtx C) D {
									if usb == "" {
										return D{}
									}
									b := material.Button(th, &fixBtn, "Restart receiver")
									b.Background = colBad
									return b.Layout(gtx)
								}),
							)
						})}
						for _, l := range report {
							rows = append(rows, layout.Rigid(func(gtx C) D {
								return layout.Inset{Top: 8}.Layout(gtx, func(gtx C) D { return diagRow(gtx, th, l) })
							}))
						}
						return layout.Flex{Axis: layout.Vertical}.Layout(gtx, rows...)
					})
				},
				func(gtx C) D {
					return layout.Inset{Left: 20, Right: 20, Top: 4, Bottom: 20}.Layout(gtx, func(gtx C) D {
						return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
							layout.Flexed(1, func(gtx C) D {
								l := material.Caption(th, appName+" "+version+map[bool]string{true: "\n" + msg}[msg != ""])
								l.Color = colMuted
								return l.Layout(gtx)
							}),
							layout.Rigid(func(gtx C) D {
								b := material.Button(th, &updBtn, "Check for updates")
								b.Background = colCard
								return b.Layout(gtx)
							}),
						)
					})
				},
			}
			material.List(th, &list).Layout(gtx, len(sections), func(gtx C, i int) D { return sections[i](gtx) })
			e.Frame(gtx.Ops)
		}
	}
}

// pick hands a device choice to the worker.
func pick(eps []endpoint, id string, ref *endpoint) {
	for _, e := range eps {
		if e.ID == id {
			jobs <- func() { *ref = e; saveConfig() }
		}
	}
}

func header(gtx C, th *material.Theme, s snapshot) D {
	pct, col := "—", colMuted
	if s.H.Online {
		pct, col = strconv.Itoa(s.H.Battery)+"%", colOK
		switch {
		case s.H.Charging:
			col = colCharge
		case s.H.Battery <= 25:
			col = colBad
		}
	} else if !s.H.Dongle {
		col = colBad
	}
	return layout.Inset{Left: 20, Right: 20, Top: 20, Bottom: 8}.Layout(gtx, func(gtx C) D {
		return layout.Flex{Alignment: layout.Middle}.Layout(gtx,
			layout.Flexed(1, func(gtx C) D {
				return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
					layout.Rigid(material.H5(th, "Arctis 7+").Layout),
					layout.Rigid(func(gtx C) D {
						l := material.Body2(th, s.Status)
						l.Color = col
						return l.Layout(gtx)
					}),
				)
			}),
			layout.Rigid(func(gtx C) D {
				l := material.H3(th, pct)
				l.Color = col
				return l.Layout(gtx)
			}),
		)
	})
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
					return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
						layout.Rigid(func(gtx C) D {
							l := material.Label(th, 13, strings.ToUpper(title))
							l.Color = colMuted
							return l.Layout(gtx)
						}),
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
				layout.Rigid(func(gtx C) D {
					l := material.Caption(th, sub)
					l.Color = colMuted
					return l.Layout(gtx)
				}),
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
