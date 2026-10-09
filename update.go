package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"runtime"
	"time"
)

const repo = "NekrosBurek/OpenSeries"

var version = "dev" // set by -ldflags "-X main.version=vX.Y.Z"

// selfUpdate replaces the running binary with the latest GitHub release asset.
// Returns the new tag, or "" when already current.
func selfUpdate() (string, error) {
	if version == "dev" {
		return "", nil
	}
	c := http.Client{Timeout: 2 * time.Minute}
	resp, err := c.Get("https://api.github.com/repos/" + repo + "/releases/latest")
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	var rel struct {
		Tag    string `json:"tag_name"`
		Assets []struct {
			Name string `json:"name"`
			URL  string `json:"browser_download_url"`
		} `json:"assets"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return "", err
	}
	if rel.Tag == "" || rel.Tag == version {
		return "", nil
	}
	want := fmt.Sprintf("openseries-%s-%s", runtime.GOOS, runtime.GOARCH)
	if runtime.GOOS == "windows" {
		want += ".exe"
	}
	for _, a := range rel.Assets {
		if a.Name == want {
			return rel.Tag, replaceSelf(&c, a.URL)
		}
	}
	return "", fmt.Errorf("release %s has no %s", rel.Tag, want)
}

func replaceSelf(c *http.Client, url string) error {
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	resp, err := c.Get(url)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return fmt.Errorf("download: %s", resp.Status)
	}
	f, err := os.OpenFile(exe+".new", os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
	if err != nil {
		return err
	}
	_, err = io.Copy(f, resp.Body)
	if cerr := f.Close(); err == nil {
		err = cerr
	}
	if err != nil {
		os.Remove(exe + ".new")
		return err
	}
	// Windows can rename a running exe but not overwrite it.
	os.Remove(exe + ".old")
	if err := os.Rename(exe, exe+".old"); err != nil {
		return err
	}
	return os.Rename(exe+".new", exe)
}
