package main

import "testing"

func TestResolve(t *testing.T) {
	eps := []endpoint{{"a", "SteelSeries Sonar - Microphone (SteelSeries Sonar Virtual Audio Device)"}, {"b", "Microphone (7- Arctis 7+)"}}
	if e, _ := resolve(eps, endpoint{"b", "x"}); e.ID != "b" {
		t.Fatal("id match")
	}
	if e, _ := resolve(eps, endpoint{"old", "Microphone (4- Arctis 7+)"}); e.ID != "b" {
		t.Fatal("name match across USB port change")
	}
	if _, ok := resolve(eps, endpoint{"old", "Microphone (Other)"}); ok {
		t.Fatal("false match")
	}
}
