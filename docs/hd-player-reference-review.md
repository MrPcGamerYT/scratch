# HD-Player reference review

Inspected 2026-11-24 from the user-supplied `HD-Player.rar` on main
(Git blob `69be9e7eda856fd1401af22b4a43c6a454c8ef6b`, 195436 bytes).
Extracted for static inspection only; no supplied code or executable was run.
Reference files remain outside the shipped source and packages.

## What the archive contains

A C# desktop host project targeting .NET Framework 3.5, with WinForms/WPF
components. Assembly metadata identifies BlueStacks Android Host 4.280.1.1002
and a BlueStack Systems copyright. Token/RVA comments suggest decompiled source;
provenance and redistribution permissions have not been established. No code,
icons or branding from this reference were copied into Nexus.

The archive is not a self-contained emulator engine. The project references
BstkTypeLib and HD-Common; VMCommand imports HD-VMCommand-Native.dll and SysPGA
imports HD-OpenGl-Native.dll. The native graphics/hypervisor implementations
are not supplied by these C# wrappers. Their presence cannot enable Nexus GPU
acceleration, Android translation, FPS control or commercial-emulator compatibility.

## Architectural observations

- VMWindow and LayoutManager coordinate window/display lifecycle.
- AndroidBootUp and StateMachine separate boot progress from the window shell.
- IGraphics exposes graphics lifecycle, orientation and capability queries;
  Opengl delegates rendering to native implementations.
- InputMapper, MacroManager, camera, media and gamepad components are separate
  subsystems rather than toolbar-only features.

## Independent Nexus improvements in this change

- Toolbar order: Back, Home, Recents, fullscreen, keymap menu, volume, APK, settings.
- Keymap menu owns Studio access, keymap enable, hints and mouse lock.
- No title-bar Aim button; aim configuration remains in Keymap Studio.
- Volume up/down/mute use Android ADB key events with the same bounded,
  asynchronous, authorized-guest prerequisites as navigation.
- Browser preview mirrors menus and clearly identifies demo-only operations.

## Remaining engineering work (not claimed complete)

A guest-capability service, structured boot diagnostics, display backend abstraction,
and measured GPU acceleration need independent implementations and testing. A
native renderer cannot be substituted merely by copying this host's function names.
Rust remains the runtime/input migration path; QEMU remains the VM/display backend.

## Additional input review

`InputMapper.cs` registers Windows raw input and forwards buffers to the native
`ImapHandleRawInput` function. The supplied wrapper does not contain the native
mapper's processing implementation, so its smoothing/latency behavior cannot be
reproduced from this file alone. Nexus currently uses Windows hooks and a 16 ms
movement-coalescing timer for aim; it does not yet have equivalent raw-input
transport or measured end-to-end latency. Independent X/Y sensitivity exists,
but changing timer rates alone is not proof of smoother or faster gameplay.
No QML files were found in the supplied archive or available chat uploads.
