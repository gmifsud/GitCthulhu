# terminal_widget

A terminal for the GitCthulhu window: a real shell on a pseudoconsole (ConPTY on Windows),
drawn by an iced canvas, with alacritty's terminal emulation.

Vendored from [iced_term](https://github.com/Harzu/iced_term) 0.6.0 under its MIT licence
(`LICENSE`). Changes from upstream:

- A click inside the terminal focuses it; a click anywhere else lets go of the keyboard,
  so typing in another field never also reaches the shell.
- `backend::Command` is public, so the window can write to the shell (Clear, Stop).
- A closed event channel ends the subscription instead of panicking.
