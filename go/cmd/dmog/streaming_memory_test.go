// Copyright (c) 2026 tabnas, MIT License

package main

// streaming_memory_test.go: the streaming gate's measured run. runOverFile
// runs this test binary again as the command: TestMain, finding
// DMOG_STREAMING_MEMORY in its environment, calls streamingHelper in place
// of the tests, which runs command(), the dmog command's own code, in this
// process, with its input and its output passed through a sampler of the
// process's live heap. What runs is the shipped command; the sampler only
// reads the heap at the points the stream passes it.

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"runtime"
	"strconv"
)

// streamingMemoryEnv names the file a measured run writes its report to,
// and streamingLimitEnv the bytes past which it stops.
const (
	streamingMemoryEnv = "DMOG_STREAMING_MEMORY"
	streamingLimitEnv  = "DMOG_STREAMING_LIMIT"
)

// streamingReport is what a measured run reports: its peak, in bytes, and
// whether it was stopped once past its limit, with how many samples it
// took.
type streamingReport struct {
	Peak    int64 `json:"peak"`
	Over    bool  `json:"over"`
	Samples int   `json:"samples"`
}

// heapSampler samples what the command holds, the live heap, at fixed
// points of the stream: before the command begins, and before every read of
// its input and every write of its output. A sample is what is allocated
// once a collection has run to its end, its sweep included (runtime.GC,
// then runtime.ReadMemStats' HeapAlloc), and the command allocates on the
// one goroutine, so what a sample finds at a point differs from run to run
// only by what the collector's own schedule between samples leaves, a few
// percent, however busy the machine. The run's peak is the most a later
// sample holds past the least any sample holds.
type heapSampler struct {
	report  string
	limit   int64
	least   int64
	most    int64
	samples int
	stats   runtime.MemStats
}

func (s *heapSampler) held() int64 {
	runtime.GC()
	runtime.ReadMemStats(&s.stats)
	return int64(s.stats.HeapAlloc)
}

func (s *heapSampler) peak() int64 { return max(0, s.most-s.least) }

func (s *heapSampler) sample() {
	s.samples++
	now := s.held()
	s.least = min(s.least, now)
	s.most = max(s.most, now)
	if s.limit > 0 && s.peak() > s.limit {
		// Past the limit, the run has shown what the gate asks of it.
		s.write(true)
		os.Exit(0)
	}
}

func (s *heapSampler) write(over bool) {
	data, err := json.Marshal(streamingReport{s.peak(), over, s.samples})
	if err == nil {
		err = os.WriteFile(s.report, data, 0o644)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "the measured run cannot report:", err)
	}
}

type sampledReader struct {
	r io.Reader
	s *heapSampler
}

func (r sampledReader) Read(p []byte) (int, error) {
	r.s.sample()
	return r.r.Read(p)
}

type sampledWriter struct {
	w io.Writer
	s *heapSampler
}

func (w sampledWriter) Write(p []byte) (int, error) {
	w.s.sample()
	return w.w.Write(p)
}

// streamingHelper runs the command over args, whose last is INPUT, a file,
// and reports to report: the file is the command's standard input ("-"),
// read through the sampler, and its standard output is written through it.
// It answers the command's status.
func streamingHelper(report string, args []string) int {
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "the measured run has no arguments")
		return 2
	}
	limit, _ := strconv.ParseInt(os.Getenv(streamingLimitEnv), 10, 64)
	input := args[len(args)-1]
	file, err := os.Open(input)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	defer file.Close()
	s := &heapSampler{report: report, limit: limit}
	s.least = s.held()
	args = append(append([]string{}, args[:len(args)-1]...), "-")
	status := command(args, sampledReader{file, s}, sampledWriter{os.Stdout, s}, os.Stderr)
	s.write(false)
	return status
}
