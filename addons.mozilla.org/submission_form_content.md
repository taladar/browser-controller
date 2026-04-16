# Firefox Addons Submission

Name: Browser Controller

Summary:

Provides a way to inspect and control browser windows and tabs from a Rust CLI
program. It also allows watching an event stream of window and tab events. The
Event stream and CLI output use JSON for simple scripting with e.g. jq

Description:

This is meant to allow a power user to integrate browser window and tab control
with other parts of their environment, e.g. shell scripts, window manager or
compositor shortcuts, system services or timed jobs.

For ease of scripting the output can optionally use JSON and the event stream of
browser events uses newline delimited JSON objects.

For windows it allows listing, opening (including private/incognito windows) and
closing windows as well as setting and removing a window title prefix
that can be
used to e.g. distinguish different Firefox windows from each other if you want
your window system rules to always move one to your left screen and the other to
your right screen or want to open work tabs in a specific window and personal
tabs in another.

For tabs it allows listing, opening, activating, navigating to a new URL,
closing, reloading (with optional cache bypass), pinning, unpinning, toggling
Reader Mode, discarding tabs to free memory, warming discarded tabs up, muting
and unmuting tab audio, moving the tab to a different position in the window's
tab bar, going forward and backward in history, sorting tabs by domain, and
reopening tabs in a different Firefox container.

For containers it allows listing all available Firefox containers (contextual
identities) and reopening tabs in a specific container.

For downloads it allows listing (with optional state and query filters),
starting
new downloads, cancelling, pausing, resuming, retrying interrupted downloads,
and erasing download history entries.

When opening a new tab it can also optionally provide credentials for HTTP basic
authentication via the browser's onAuthRequired API, avoiding the need to embed
credentials in the URL where they would be visible in the address bar, history,
and logs.

The event stream includes events for window and tab operations (open, close,
activate, navigate, title change, status change) as well as download events
(created, changed, erased).

Note: My primary testing and development platform for this is Linux but I do
provide binaries for other desktop platforms for the Rust side of this. Bug
reports from users on those platforms are welcome if I overlooked some minor
platform-specific issues. This is not really meant for use on mobile platforms
since CLI use is uncommon there.

Category: Tabs

Support website: <https://github.com/taladar/browser-controller>

License: Apache License 2.0

Privacy Policy:

No data is collected or sent to the developer by this add-on.

However the add-on does provide access to some data from your browser session to
the CLI you install on your own machine which displays it to you or anyone else
who can call it under your user account or a privileged account on your local
system which can access the file in your user directory.

Notes to Reviewer:

You can find the source code at <https://github.com/taladar/browser-controller>
(with version tags) and there are published release binaries for mediator and
CLI there and the Rust packages are also published to crates.io for install
via cargo.

My primary testing platform was Linux so I do expect other platforms like
Windows or macOS that I did not have available for testing might still have
small errors as I only cross-compiled for them but did not have a native system
for testing.
