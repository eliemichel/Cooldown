extern crate midir;

use std::io::{stdin, stdout, Write};
use midir::{MidiOutput,MidiInput};

use std::error::Error;

pub fn get_midi_in() -> Result<(MidiInput, usize),Box<Error>> {
    let midi_in = MidiInput::new("Cooldown MIDI Input")?;

    let in_port = match midi_in.port_count() {
        0 => return Err("no input MIDI device found".into()),
        1 => {
            println!("Choosing the only available input device: {}", midi_in.port_name(0).unwrap());
            0
        },
        _ => {
            println!("\nAvailable MIDI input devices:");
            for i in 0..midi_in.port_count() {
                println!("{}: {}", i, midi_in.port_name(i).unwrap());
            }
            print!("Please select input device: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            input.trim().parse()?
        }
    };

    Ok((midi_in, in_port))
}

pub fn get_midi_out() -> Result<(MidiOutput, usize),Box<Error>> {
    let midi_out = MidiOutput::new("My Test Output")?;    

    // Get an output port (read from console if multiple are available)
    let out_port = match midi_out.port_count() {
        0 => return Err("no output MIDI device found".into()),
        1 => {
            println!("Choosing the only available output MIDI device: {}", midi_out.port_name(0).unwrap());
            0
        },
        _ => {
            println!("\nAvailable output MIDI devices:");
            for i in 0..midi_out.port_count() {
                println!("{}: {}", i, midi_out.port_name(i).unwrap());
            }
            print!("Please select output device: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            input.trim().parse()?
        }
    };

    Ok((midi_out, out_port))
}
