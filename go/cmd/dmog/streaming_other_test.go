// Copyright (c) 2026 tabnas, MIT License

//go:build !unix

package main

import "testing"

// makeFIFO answers no FIFO: mkfifo is POSIX, so the streaming gate runs
// standard input alone here.
func makeFIFO(t *testing.T, dir, id string) string { return "" }

func releaseFIFO(fifo string) {}
