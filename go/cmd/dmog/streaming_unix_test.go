// Copyright (c) 2026 tabnas, MIT License

//go:build unix

package main

import (
	"path/filepath"
	"syscall"
	"testing"
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
