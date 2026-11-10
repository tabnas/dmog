// Copyright (c) 2026 tabnas, MIT License

//go:build unix

package main

import (
	"bufio"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

// makeFIFO makes a named pipe in dir for the streaming gate's file route.
func makeFIFO(t *testing.T, dir, id string) string {
	t.Helper()
	path := filepath.Join(dir, id+".fifo")
	if err := syscall.Mkfifo(path, 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

// releaseFIFO opens the FIFO to read without waiting, and closes it, so that
// a writer waiting in its open goes on, to a write that fails.
func releaseFIFO(fifo string) {
	if fifo == "" {
		return
	}
	if fd, err := syscall.Open(fifo, syscall.O_RDONLY|syscall.O_NONBLOCK, 0); err == nil {
		syscall.Close(fd)
	}
}

// peakOf is the exited command's peak resident set, in bytes: getrusage's
// ru_maxrss, in kilobytes on Linux and in bytes on macOS.
func peakOf(state *os.ProcessState) (int64, bool) {
	usage, ok := state.SysUsage().(*syscall.Rusage)
	if !ok {
		return 0, false
	}
	if runtime.GOOS == "darwin" {
		return usage.Maxrss, true
	}
	return usage.Maxrss * 1024, true
}

// watchPeak reads the running command's peak resident set (VmHWM) on Linux
// every 20 ms until done, and calls kill once it is past limit bytes,
// answering the peak it read then, or 0 where it did not. Elsewhere, or with
// no limit, it waits for done.
func watchPeak(pid int, limit int64, done <-chan struct{}, kill func() error) int64 {
	if limit <= 0 || runtime.GOOS != "linux" {
		<-done
		return 0
	}
	status := "/proc/" + strconv.Itoa(pid) + "/status"
	tick := time.NewTicker(20 * time.Millisecond)
	defer tick.Stop()
	for {
		select {
		case <-done:
			return 0
		case <-tick.C:
			if hwm, ok := vmHWM(status); ok && hwm > limit {
				kill()
				<-done
				return hwm
			}
		}
	}
}

// vmHWM is the VmHWM line of a /proc status file, in bytes.
func vmHWM(path string) (int64, bool) {
	f, err := os.Open(path)
	if err != nil {
		return 0, false
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		if rest, ok := strings.CutPrefix(line, "VmHWM:"); ok {
			fields := strings.Fields(rest)
			if len(fields) == 0 {
				return 0, false
			}
			kb, err := strconv.ParseInt(fields[0], 10, 64)
			return kb * 1024, err == nil
		}
	}
	return 0, false
}
