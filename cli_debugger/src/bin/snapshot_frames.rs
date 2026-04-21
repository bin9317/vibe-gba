use cli_debugger::debug_support::{
    diff_pixels, load_gba_with_bios, load_gba_with_state, step_frames, write_frame_image,
    ImageFormat,
};
use gba_core::Gba;
use std::fs;
use std::path::PathBuf;

struct Config {
    rom_path: String,
    output_dir: PathBuf,
    bios_path: String,
    state_path: Option<String>,
    image_format: ImageFormat,
    snapshot_every_frames: u32,
    max_frames: u32,
    stall_threshold_pixels: usize,
    stall_limit: u32,
    skip_bios: bool,
    capture_initial_frame: bool,
    input_events: Vec<InputEvent>,
}

#[derive(Clone, Copy)]
struct InputEvent {
    frame: u32,
    mask: u16,
    hold_frames: u32,
}

fn main() {
    let config = parse_args();
    fs::create_dir_all(&config.output_dir).expect("failed to create output directory");

    let mut gba = if let Some(state_path) = config.state_path.as_deref() {
        load_gba_with_state(&config.rom_path, Some(&config.bios_path), state_path)
            .expect("failed to initialize gba from state")
    } else {
        load_gba_with_bios(&config.rom_path, Some(&config.bios_path), config.skip_bios)
            .expect("failed to initialize gba")
    };

    let mut previous_frame: Option<Vec<u8>> = None;
    let mut consecutive_still_frames = 0u32;

    if config.capture_initial_frame {
        write_snapshot(
            &config,
            &gba,
            0,
            gba.bus.ppu.frame_buffer.to_vec(),
            gba.bus.ppu.frame_buffer.len() / 4,
            0,
        );
        previous_frame = Some(gba.bus.ppu.frame_buffer.to_vec());
    }

    for frame in 0..config.max_frames {
        apply_input_events(&mut gba, &config.input_events, frame);
        step_frames(&mut gba, 1);

        if frame % config.snapshot_every_frames != 0 {
            continue;
        }

        let current_frame = gba.bus.ppu.frame_buffer.to_vec();
        let changed_pixels = previous_frame
            .as_ref()
            .map(|previous| diff_pixels(previous, &current_frame))
            .unwrap_or(current_frame.len() / 4);

        if changed_pixels <= config.stall_threshold_pixels {
            consecutive_still_frames += 1;
        } else {
            consecutive_still_frames = 0;
        }

        write_snapshot(
            &config,
            &gba,
            frame,
            current_frame.clone(),
            changed_pixels,
            consecutive_still_frames,
        );

        if consecutive_still_frames >= config.stall_limit {
            println!(
                "screen appears stalled after {} snapshots at frame {}",
                consecutive_still_frames, frame
            );
        }

        previous_frame = Some(current_frame);
    }
}

fn parse_args() -> Config {
    let mut rom_path = "tests/roms/mario.gba".to_string();
    let mut output_dir = PathBuf::from("snapshots");
    let mut bios_path = "gba_bios.bin".to_string();
    let mut state_path = None;
    let mut image_format = ImageFormat::Png;
    let mut snapshot_every_frames = 60u32;
    let mut max_frames = 3600u32;
    let mut stall_threshold_pixels = 16usize;
    let mut stall_limit = 5u32;
    let mut skip_bios = false;
    let mut capture_initial_frame = false;
    let mut input_events = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output-dir" => {
                output_dir = PathBuf::from(args.next().expect("missing value for --output-dir"))
            }
            "--bios" => bios_path = args.next().expect("missing value for --bios"),
            "--load-state" => {
                state_path = Some(args.next().expect("missing value for --load-state"))
            }
            "--image-format" => {
                image_format = match args
                    .next()
                    .expect("missing value for --image-format")
                    .as_str()
                {
                    "bmp" => ImageFormat::Bmp,
                    "png" => ImageFormat::Png,
                    other => panic!("unsupported --image-format: {other}"),
                };
            }
            "--snapshot-every-frames" => {
                snapshot_every_frames = args
                    .next()
                    .expect("missing value for --snapshot-every-frames")
                    .parse()
                    .expect("invalid --snapshot-every-frames");
            }
            "--max-frames" => {
                max_frames = args
                    .next()
                    .expect("missing value for --max-frames")
                    .parse()
                    .expect("invalid --max-frames");
            }
            "--stall-threshold-pixels" => {
                stall_threshold_pixels = args
                    .next()
                    .expect("missing value for --stall-threshold-pixels")
                    .parse()
                    .expect("invalid --stall-threshold-pixels");
            }
            "--stall-limit" => {
                stall_limit = args
                    .next()
                    .expect("missing value for --stall-limit")
                    .parse()
                    .expect("invalid --stall-limit");
            }
            "--press-a-at-frame" => {
                let frame = args
                    .next()
                    .expect("missing value for --press-a-at-frame")
                    .parse()
                    .expect("invalid --press-a-at-frame");
                let hold_frames = args
                    .next()
                    .unwrap_or_else(|| "2".to_string())
                    .parse()
                    .expect("invalid hold frames for --press-a-at-frame");
                input_events.push(InputEvent {
                    frame,
                    mask: 1 << 0,
                    hold_frames,
                });
            }
            "--press-start-at-frame" => {
                let frame = args
                    .next()
                    .expect("missing value for --press-start-at-frame")
                    .parse()
                    .expect("invalid --press-start-at-frame");
                let hold_frames = args
                    .next()
                    .unwrap_or_else(|| "2".to_string())
                    .parse()
                    .expect("invalid hold frames for --press-start-at-frame");
                input_events.push(InputEvent {
                    frame,
                    mask: 1 << 3,
                    hold_frames,
                });
            }
            "--press-mask-at-frame" => {
                let frame = args
                    .next()
                    .expect("missing value for --press-mask-at-frame")
                    .parse()
                    .expect("invalid --press-mask-at-frame");
                let mask = u16::from_str_radix(
                    &args
                        .next()
                        .expect("missing key mask for --press-mask-at-frame")
                        .trim_start_matches("0x"),
                    16,
                )
                .expect("invalid key mask for --press-mask-at-frame");
                let hold_frames = args
                    .next()
                    .unwrap_or_else(|| "2".to_string())
                    .parse()
                    .expect("invalid hold frames for --press-mask-at-frame");
                input_events.push(InputEvent {
                    frame,
                    mask,
                    hold_frames,
                });
            }
            "--skip-bios" => skip_bios = true,
            "--capture-initial-frame" => capture_initial_frame = true,
            _ if arg.starts_with("--") => panic!("unknown option: {arg}"),
            _ => rom_path = arg,
        }
    }

    Config {
        rom_path,
        output_dir,
        bios_path,
        state_path,
        image_format,
        snapshot_every_frames,
        max_frames,
        stall_threshold_pixels,
        stall_limit,
        skip_bios,
        capture_initial_frame,
        input_events,
    }
}

fn apply_input_events(gba: &mut Gba, events: &[InputEvent], frame: u32) {
    gba.bus.key_state = 0x03FF;

    for event in events {
        if frame >= event.frame && frame < event.frame + event.hold_frames {
            gba.bus.key_state &= !event.mask;
        }
    }
}

fn write_snapshot(
    config: &Config,
    gba: &Gba,
    frame: u32,
    current_frame: Vec<u8>,
    changed_pixels: usize,
    consecutive_still_frames: u32,
) {
    let image_path = config.output_dir.join(format!(
        "frame_{frame:06}.{}",
        config.image_format.extension()
    ));
    write_frame_image(&image_path, &current_frame).expect("failed to write image");

    println!(
        "frame={frame} pc={:08X} changed_pixels={} consecutive_still={} image={}",
        gba.cpu.registers[15],
        changed_pixels,
        consecutive_still_frames,
        image_path.display(),
    );
}
