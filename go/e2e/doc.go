// Copyright (c) 2026 tabnas, MIT License

// Package e2e holds alchemy's end-to-end tests: programs compiled by
// github.com/tabnas/alchemy/go and run on transduce's Routers and render's
// Renderers, the stages the dmog command composes. alchemy depends on
// neither (a host hands Compile both), so the tests that run a program, or
// hold it to what transduce or render do themselves, live here, in the
// composition root that builds all three; alchemy's own tests stop at the
// plan. They were alchemy's go/*_test.go, and each file here keeps the name
// of the file it came from.
//
// The tests are black-box, through alchemy's exported API, which they
// import with a dot so that they read as they did inside the package. They
// read alchemy's fixtures from its checkout beside this one
// (../alchemy/test/spec) and transduce's from transduce's
// (../transduce/rs/tests/fixtures); a missing sibling fails the test that
// needs it, naming the path. The package has no code of its own.
package e2e
