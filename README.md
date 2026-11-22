<div align="center">

# omarchy-display-brightness

![Rust](https://img.shields.io/badge/Rust-2024-d99a6c?style=flat-square&logo=rust&logoColor=white)
![Platform](https://img.shields.io/badge/Omarchy%20%2B%20Hyprland-1a79a5?style=flat-square)
![License](https://img.shields.io/badge/License-MIT-3da639?style=flat-square)

</div>

Display brightness for [Omarchy](https://omarchy.org) that keeps working when
the monitor does not answer DDC/CI.

---

## The problem

An external monitor that ignores DDC/CI cannot be dimmed through hardware.
Omarchy's `omarchy-brightness-display` detects that and exits non-zero, so
the brightness keys silently do nothing:

```
$ ddcutil getvcp 10
No displays implementing DDC/CI found
```

`wl-gammarelay-rs` dims the picture in software instead, and it works on
every monitor. Omarchy already uses it for its fake-blank (screen off), but
not for the `+5%` path.

## What this does

1. Tries the hardware path (`omarchy-brightness-display`), which also draws
   the OSD.
2. If that fails, remembers the failure for a minute and switches to
   `wl-gammarelay-rs` for the rest of the press.

The one-minute cache matters: probing DDC costs a few hundred milliseconds
and the brightness keys repeat, so without it every press would pay for a
probe that cannot succeed.

## Install

```sh
cargo install --git https://github.com/windy664/omarchy-display-brightness
```

The CI builds the binary and attaches it to every run, so a machine without a
Rust toolchain can install it from the artifact instead:

```sh
gh run download --repo windy664/omarchy-display-brightness \
  --name omarchy-display-brightness-x86_64-unknown-linux-gnu \
  -D .
install -Dm755 display-brightness ~/.local/bin/display-brightness
```

## Use

```sh
display-brightness +3%     # brighter by 3 percent
display-brightness 3%-     # dimmer by 3 percent
display-brightness 100%    # all the way up
display-brightness 1%      # all the way down
```

Bind it in `~/.config/hypr/bindings.lua`:

```lua
o.bind("XF86MonBrightnessUp", "Brightness up", "display-brightness +3%", { repeating = true })
o.bind("XF86MonBrightnessDown", "Brightness down", "display-brightness 3%-", { repeating = true })
o.bind("SHIFT + XF86MonBrightnessUp", "Brightness max", "display-brightness 100%", { repeating = true })
o.bind("SHIFT + XF86MonBrightnessDown", "Brightness min", "display-brightness 1%", { repeating = true })
```

## Notes

- The binary is called `display-brightness` so it can replace the shell
  script of the same name without touching the keybindings.
- It shells out to `busctl`, `pgrep` and `omarchy-osd` rather than talking to
  D-Bus itself, which keeps the crate dependency-free and in step with
  whatever the rest of the desktop expects.
- The software path is a gamma transform, not a backlight: it dims what the
  compositor draws, so it does not change the monitor's own brightness and
  the screen is still lit.

## License

MIT — see [LICENSE](LICENSE).
