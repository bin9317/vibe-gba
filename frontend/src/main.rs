mod config;
mod save;

use gba_core::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};
use gba_core::Gba;
use gba_core::GBA_CYCLES_PER_FRAME;
use pixels::{Pixels, SurfaceTexture};
use std::rc::Rc;
use std::{
    fs,
    path::{Path, PathBuf},
};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, ModifiersState, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn state_path_for_rom_slot(rom_path: &Path, slot: u8) -> PathBuf {
    if slot == 0 {
        PathBuf::from(format!("{}.state", rom_path.to_string_lossy()))
    } else {
        PathBuf::from(format!("{}.state.{}", rom_path.to_string_lossy(), slot))
    }
}

fn slot_for_key(key: VirtualKeyCode) -> Option<u8> {
    Some(match key {
        VirtualKeyCode::Key0 => 0,
        VirtualKeyCode::Key1 => 1,
        VirtualKeyCode::Key2 => 2,
        VirtualKeyCode::Key3 => 3,
        VirtualKeyCode::Key4 => 4,
        VirtualKeyCode::Key5 => 5,
        VirtualKeyCode::Key6 => 6,
        VirtualKeyCode::Key7 => 7,
        VirtualKeyCode::Key8 => 8,
        VirtualKeyCode::Key9 => 9,
        _ => return None,
    })
}

fn save_state_to_slot(gba: &Gba, rom_path: &Path, slot: u8) {
    let state_path = state_path_for_rom_slot(rom_path, slot);
    match gba.save_state() {
        Ok(bytes) => match save::atomic_write(&state_path, &bytes) {
            Ok(()) => println!("Saved state slot {} to {}", slot, state_path.display()),
            Err(err) => eprintln!("Failed to write state {}: {}", state_path.display(), err),
        },
        Err(err) => eprintln!("Failed to serialize state: {}", err),
    }
}

fn load_state_from_slot(gba: &mut Gba, rom_path: &Path, slot: u8) {
    let state_path = state_path_for_rom_slot(rom_path, slot);
    match fs::read(&state_path) {
        Ok(bytes) => match gba.load_state(&bytes) {
            Ok(()) => println!("Loaded state slot {} from {}", slot, state_path.display()),
            Err(err) => eprintln!("Failed to load state {}: {}", state_path.display(), err),
        },
        Err(err) => eprintln!("Failed to read state {}: {}", state_path.display(), err),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rom_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/roms/armwrestler.gba".to_string()
    };
    let rom_path = std::path::PathBuf::from(rom_path);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("Vibe GBA - {}", rom_path.display()))
        .with_inner_size(LogicalSize::new(
            (SCREEN_WIDTH * 3) as f64,
            (SCREEN_HEIGHT * 3) as f64,
        ))
        .build(&event_loop)
        .unwrap();

    let window = Rc::new(window);

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture =
            SurfaceTexture::new(window_size.width, window_size.height, window.as_ref());
        Pixels::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, surface_texture).unwrap()
    };

    let mut gba = Gba::new();

    // Require a real BIOS for normal frontend runs. The skip-BIOS path is kept in
    // debugger tools only, where imperfect hardware initialization is explicit.
    let bios = match std::fs::read("gba_bios.bin") {
        Ok(bios) => bios,
        Err(e) => {
            eprintln!("Failed to load gba_bios.bin: {e}");
            eprintln!("Place a legally obtained GBA BIOS at ./gba_bios.bin");
            std::process::exit(1);
        }
    };
    gba.bus.load_bios(&bios);
    println!("Loaded gba_bios.bin");

    // Load ROM
    match std::fs::read(&rom_path) {
        Ok(rom) => {
            gba.bus.load_rom(&rom);
            println!("Loaded {}", rom_path.display());
        }
        Err(e) => {
            eprintln!("Failed to load ROM {}: {}", rom_path.display(), e);
            return;
        }
    }

    if let Err(e) = save::load_save(&mut gba.bus, &rom_path) {
        eprintln!("Failed to load save data: {}", e);
    }

    let key_bindings = config::KeyBindings::load();
    let mut current_state_slot = 0u8;
    let mut modifiers = ModifiersState::default();
    println!("GBA Bindings: A={:?}, B={:?}, L={:?}, R={:?}, Select={:?}, Start={:?}, Up={:?}, Down={:?}, Left={:?}, Right={:?}",
        key_bindings.a, key_bindings.b, key_bindings.l, key_bindings.r,
        key_bindings.select, key_bindings.start,
        key_bindings.up, key_bindings.down, key_bindings.left, key_bindings.right
    );
    println!(
        "State hotkeys: save={:?} load={:?} slot={} path={}",
        key_bindings.save_state,
        key_bindings.load_state,
        current_state_slot,
        state_path_for_rom_slot(&rom_path, current_state_slot).display()
    );
    println!("State slots: Cmd+0..9 selects slot, Cmd+S saves, Cmd+L loads");

    // gba.cpu.skip_bios(&mut gba.bus);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    if let Err(e) = save::save_dirty_data(&mut gba.bus, &rom_path) {
                        eprintln!("Failed to save game data: {}", e);
                    }
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(size) => {
                    if let Err(err) = pixels.resize_surface(size.width, size.height) {
                        eprintln!("pixels.resize_surface() failed: {err}");
                        *control_flow = ControlFlow::Exit;
                    }
                }
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    modifiers = new_modifiers;
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(key) = input.virtual_keycode {
                        if input.state == ElementState::Pressed {
                            let command_like = modifiers.logo() || modifiers.ctrl();
                            if command_like {
                                if let Some(slot) = slot_for_key(key) {
                                    current_state_slot = slot;
                                    let state_path =
                                        state_path_for_rom_slot(&rom_path, current_state_slot);
                                    println!(
                                        "Selected state slot {} ({})",
                                        current_state_slot,
                                        state_path.display()
                                    );
                                    return;
                                }
                            }
                            if command_like && key == winit::event::VirtualKeyCode::S {
                                save_state_to_slot(&gba, &rom_path, current_state_slot);
                                return;
                            }
                            if command_like && key == winit::event::VirtualKeyCode::L {
                                load_state_from_slot(&mut gba, &rom_path, current_state_slot);
                                return;
                            }
                            if key == key_bindings.save_state {
                                save_state_to_slot(&gba, &rom_path, current_state_slot);
                                return;
                            }
                            if key == key_bindings.load_state {
                                load_state_from_slot(&mut gba, &rom_path, current_state_slot);
                                return;
                            }
                        }

                        if let Some(bit) = key_bindings.button_bit(key) {
                            if input.state == ElementState::Pressed {
                                gba.bus.key_state &= !(1 << bit);
                            } else {
                                gba.bus.key_state |= 1 << bit;
                            }
                        }
                    }
                }
                _ => (),
            },
            Event::RedrawRequested(_) => {
                let frame = pixels.frame_mut();
                // frame is RGBA, ppu.frame_buffer is also RGBA
                frame.copy_from_slice(gba.bus.ppu.frame_buffer.as_ref());
                if let Err(_) = pixels.render() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared => {
                // GBA Clock is 16.78 MHz. 16,777,216 cycles per second.
                // At 60 FPS, that's ~279,620 cycles per frame.
                let start_cycles = gba.bus.cycles;
                while (gba.bus.cycles - start_cycles) < GBA_CYCLES_PER_FRAME {
                    gba.step();
                }

                window.request_redraw();
            }
            _ => (),
        }
    });
}
