# Ragdoll Sandbox

A small GTA IV-inspired physics sandbox. One simulated person stands in a test
map; you walk around in third person and shoot, throw things at, blow up or
shoulder-barge them. Built with [Bevy](https://bevy.org) (game engine) and
[Rapier](https://rapier.rs) (physics), in Rust.

See `PLAN.md` for the design, `HOW_IT_WORKS.md` for a plain-language
explanation of the physics, and `PROGRESS.md` for what's done so far.

## Install (Arch Linux)

1. **Rust** (the language and its build tool, `cargo`):
   ```sh
   sudo pacman -S rustup
   rustup default stable
   ```
   The project pins its exact Rust version in `rust-toolchain.toml`; `rustup`
   downloads it automatically on first build.

2. **System libraries** Bevy needs for graphics, sound, input and windows:
   ```sh
   sudo pacman -S --needed base-devel alsa-lib systemd-libs vulkan-icd-loader \
       libxkbcommon libxkbcommon-x11 wayland libx11 libxcursor libxrandr libxi
   ```

3. **Vulkan driver for your GPU** (pick one):
   ```sh
   sudo pacman -S vulkan-radeon   # AMD
   sudo pacman -S vulkan-intel    # Intel
   sudo pacman -S nvidia-utils    # NVIDIA (proprietary driver)
   ```

4. **Get the code and run it:**
   ```sh
   git clone https://github.com/audunleganger/ragdoll-sandbox
   cd ragdoll-sandbox
   cargo run
   ```
   ⚠️ The **first build takes 5–15 minutes**: it compiles Bevy and Rapier from
   source. After that, rebuilding after a change takes seconds.

## Controls

Click the window to capture the mouse; **Esc** releases it.

| Key | Action |
|---|---|
| W A S D | Move |
| Shift | Sprint (run into the person to shoulder-barge them) |
| Space | Jump |
| Mouse | Look / aim |
| Left mouse | Shoot |
| 1 / 2 | Pistol / shotgun |
| E | Explosion where the crosshair points |
| Hold right mouse | Charge a throw, release to throw |
| Q | Switch throwable: light ball / heavy crate |
| R | Reset the person to the start |
| F | Spawn the person a few metres in front of you (e.g. on the platform) |
| T | Toggle slow motion |
| G | Muscles on / off (go limp) |
| − / = | Muscle tone down / up |

## Tests

```sh
cargo test
```

Runs the physics without a window and checks that the body behaves: joints
stay connected and bend the right way, the body settles instead of jittering,
and so on.
