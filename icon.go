package main

import (
	"bytes"
	"encoding/binary"
	"image"
	"image/color"
	"image/png"
	"runtime"
)

// batteryIcon draws a 32x32 battery: fill = level, blue = charging, grey = headset off, red frame = no receiver.
func batteryIcon(h headset) []byte {
	img := image.NewNRGBA(image.Rect(0, 0, 32, 32))
	rect := func(x0, y0, x1, y1 int, c color.Color) {
		for x := x0; x < x1; x++ {
			for y := y0; y < y1; y++ {
				img.Set(x, y, c)
			}
		}
	}
	frame := color.NRGBA{220, 220, 220, 255}
	if !h.Dongle {
		frame = color.NRGBA{230, 60, 60, 255}
	}
	rect(1, 7, 28, 25, frame)
	rect(28, 12, 31, 20, frame)
	rect(3, 9, 26, 23, color.NRGBA{30, 30, 30, 255})
	if h.Online {
		fill := color.NRGBA{60, 200, 90, 255}
		switch {
		case h.Charging:
			fill = color.NRGBA{60, 150, 240, 255}
		case h.Battery <= 25:
			fill = color.NRGBA{230, 60, 60, 255}
		case h.Battery <= 50:
			fill = color.NRGBA{240, 190, 40, 255}
		}
		rect(4, 10, 4+max(h.Battery, 8)*21/100, 22, fill)
	}
	var b bytes.Buffer
	png.Encode(&b, img)
	if runtime.GOOS != "windows" {
		return b.Bytes()
	}
	// Windows tray wants .ico; an ICO may wrap a PNG directly.
	ico := []byte{0, 0, 1, 0, 1, 0, 32, 32, 0, 0, 1, 0, 32, 0}
	ico = binary.LittleEndian.AppendUint32(ico, uint32(b.Len()))
	ico = binary.LittleEndian.AppendUint32(ico, 22)
	return append(ico, b.Bytes()...)
}
