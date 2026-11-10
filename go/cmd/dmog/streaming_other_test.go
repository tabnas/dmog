// Copyright (c) 2026 tabnas, MIT License

//go:build !unix

package main

import (
	"os"
	"testing"
)

// makeFIFO answers no FIFO: mkfifo is POSIX, so the streaming gate runs
// standard input alone here.
func makeFIFO(t *testing.T, dir, id string) string { return "" }

func releaseFIFO(fifo string) {}

// peakOf answers no peak: getrusage's ru_maxrss is unix's, so the streaming
// gate's memory clause is not measured here.
func peakOf(state *os.ProcessState) (int64, bool) { return 0, false }

// watchPeak waits for done: there is no peak to watch here.
func watchPeak(pid int, limit int64, done <-chan struct{}, kill func() error) int64 {
	<-done
	return 0
}
