extern crate midir;
extern crate rosc;

use std::thread;
use std::sync::mpsc;
use std::sync::{Mutex, Arc};

use std::time::{Instant,Duration};
use std::io::{stdin};
use std::error::Error;

use std::env;
use std::net::{UdpSocket, SocketAddrV4};
use rosc::{OscPacket, OscMessage, OscType};
use rosc::encoder;

use midir::{MidiOutput,MidiInput};

use std::str::FromStr;

pub mod taptempo;
pub mod midi_utils;

use taptempo::TapTempo;

const SLEEP_TIME: u64 = 10;
const BEAT_COUNT: usize = 8;

enum MessageForOscThread {
    Terminate,
    UpdateBeat(usize, f32, f32),
    UpdateVelocity(usize, f32),
    UpdateMasterVelocity(f32),
}

enum MessageForMidiThread {
    Terminate,
    UpdateBeat(usize, f32, f32),
    SendNoteOn(u8, u8),
}

struct OscSettings {
    host_addr: SocketAddrV4,
    to_addr: SocketAddrV4,
}

struct MidiSettings {
    midi_in: MidiInput,
    midi_out: MidiOutput,
    in_port: usize,
    out_port: usize,
}

struct Beat {
    frequency: f32,
    phase: f32,
    velocity: f32,
}

impl Beat {
    fn new() -> Beat {
        Beat{
            frequency: 1.0,
            phase: 1.0,
            velocity: 1.0,
        }
    }

    fn eval(&self, timer: &Arc<Mutex<Instant>>) -> f32 {
        let t = {
            let timer = timer.lock().unwrap();
            timer.elapsed().as_millis() as f32 / 1000.0
        };
        ((t - self.phase) * self.frequency).fract()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let usage = format!(
        "Usage: {} CLIENT_IP:CLIENT_PORT",
        &args[0]
    );
    if args.len() < 2 {
        panic!(usage);
    }
    let host_addr = get_addr_from_arg("127.0.0.1:0");
    let to_addr = get_addr_from_arg(&args[1]);
    let osc_settings = OscSettings{ host_addr, to_addr };

    // Prompt midi settings

    let (midi_in, in_port) = midi_utils::get_midi_in().unwrap();
    let (midi_out, out_port) = midi_utils::get_midi_out().unwrap();
    let midi_settings = MidiSettings{ midi_in, midi_out, in_port, out_port };

    // Launch threads

    let (tx_osc, rx_osc) = mpsc::channel();
    let (tx_midi, rx_midi) = mpsc::channel();
    let timer = Arc::new(Mutex::new(Instant::now()));

    let osc_thread = {
        let timer = Arc::clone(&timer);
        thread::spawn(|| run_osc_thread(osc_settings, rx_osc, timer))
    };

    let midi_thread = {
        let timer = Arc::clone(&timer);
        let tx_osc = mpsc::Sender::clone(&tx_osc);
        let tx_midi = mpsc::Sender::clone(&tx_midi);
        thread::spawn(|| match run_midi_thread(midi_settings, rx_midi, tx_midi, tx_osc, timer) {
            Ok(_) => (),
            Err(err) => println!("Error: {}", err.description())
        })
    };

    println!("\nPress <return> to exit...");
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap(); // wait for next enter key press
    tx_osc.send(MessageForOscThread::Terminate).unwrap();
    tx_midi.send(MessageForMidiThread::Terminate).unwrap();

    midi_thread.join().unwrap();
    osc_thread.join().unwrap();
}

fn run_midi_thread(midi_settings: MidiSettings, rx: mpsc::Receiver<MessageForMidiThread>, tx_midi: mpsc::Sender<MessageForMidiThread>, tx_osc: mpsc::Sender<MessageForOscThread>, timer: Arc<Mutex<Instant>>) -> Result<(), Box<Error>> {
    let MidiSettings{midi_in, in_port, midi_out, out_port} = midi_settings;
    
    println!("\nOpening connection");
    let mut conn_out = midi_out.connect(out_port, "Cooldown")?;


    let _conn_in = {
        let timer = Arc::clone(&timer);
        let tx_osc = mpsc::Sender::clone(&tx_osc);
        let mut tapper: Vec<TapTempo> = (0..BEAT_COUNT).map(|_| TapTempo::new()).collect();
        midi_in.connect(in_port, "Cooldown", move |stamp, message, _| {
            match message[0] {
                144 => { // note down
                    //conn_out.send(message).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                    match message[1] {
                        1 | 4 | 7 | 10 | 13 | 16 | 19 | 22 => { // Tap
                            let beat_index = ((message[1] - 1) / 3) as usize;
                            tap(&mut tapper[beat_index], &timer);
                            tx_osc.send(MessageForOscThread::UpdateBeat(beat_index, tapper[beat_index].frequency, tapper[beat_index].phase)).unwrap();
                            tx_midi.send(MessageForMidiThread::UpdateBeat(beat_index, tapper[beat_index].frequency, tapper[beat_index].phase)).unwrap();
                            // turn off reset button
                            tx_midi.send(MessageForMidiThread::SendNoteOn(3 + 3 * beat_index as u8, 0)).unwrap();
                        },
                        3 | 6 | 9 | 12 | 15 | 18 | 21 | 24 => { // Reset
                            let beat_index = ((message[1] - 3) / 3) as usize;
                            tapper[beat_index].reset();
                            println!("Reset tap tempo");
                            //conn_out.send(message).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                            tx_midi.send(MessageForMidiThread::SendNoteOn(message[1], 127)).unwrap();
                        },
                        _ => ()
                    }
                },
                128 => { // note up
                    //conn_out.send(&vec![144, message[1], 0]).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                },
                176 => { // knob
                    match message[1] {
                        19 | 23 | 27 | 31 | 49 | 53 | 57 | 61 => {
                            let beat_index = if message[1] < 40 { (message[1] - 19) / 4} else { 4 + (message[1] - 49) / 4 } as usize;
                            tx_osc.send(MessageForOscThread::UpdateVelocity(beat_index, message[2] as f32 / 127.0)).unwrap();
                        },
                        62 => {
                            tx_osc.send(MessageForOscThread::UpdateMasterVelocity(message[2] as f32 / 127.0)).unwrap();
                        }
                        _ => ()
                    }
                }
                _ => ()
            }
            println!("{}: {:?} (len = {})", stamp, message, message.len());
        }, ())?
    };

    let mut beat: Vec<Beat> = (0..BEAT_COUNT).map(|_| Beat::new()).collect();
    let mut bop: Vec<bool> = (0..BEAT_COUNT).map(|_| false).collect();

    'main: loop {
        // Handle events
        while let Ok(msg) = rx.try_recv() {
            match msg {
                MessageForMidiThread::Terminate => break 'main,
                MessageForMidiThread::UpdateBeat(beat_index, new_freq, new_phase) => {
                    beat[beat_index].frequency = new_freq;
                    beat[beat_index].phase = new_phase;
                },
                MessageForMidiThread::SendNoteOn(note, velocity) => {
                    conn_out.send(&vec![144, note, velocity]).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                }
            }
        }

        // Update LED feedback on MIDI controller
        for i in 0..BEAT_COUNT {
            let value = beat[i].eval(&timer);
            let bip = value < 0.5;
            if bip != bop[i] {
                let n = (1 + 3 * i) as u8;
                conn_out.send(&vec![144, n, if bip {127} else {0}]).unwrap_or_else(|_| println!("Error when forwarding message ..."));
            }
            bop[i] = bip;
        }

        thread::sleep(Duration::from_millis(SLEEP_TIME));
    }

    // Turn off buttons
    for i in 0..BEAT_COUNT {
        let n = (1 + 3 * i) as u8;
        conn_out.send(&vec![144, n, 0]).unwrap_or_else(|_| println!("Error when forwarding message ..."));
        conn_out.send(&vec![144, n + 2, 0]).unwrap_or_else(|_| println!("Error when forwarding message ..."));
    }

    println!("Closing connections");
    Ok(())
}

fn tap(tapper: &mut TapTempo, timer: &Arc<Mutex<Instant>>) {
    let t = {
        let timer = timer.lock().unwrap();
        timer.elapsed().as_millis() as f32 / 1000.0
    };
    //if (tapper.sample_count > 1 && t - lastTap > recordingMargin / est.frequency)
    tapper.add_sample(t);
    tapper.estimate();
    println!("Tap Tempo: freq = {}, phase = {}", tapper.frequency, tapper.phase);
}

fn run_osc_thread(osc_settings: OscSettings, rx: mpsc::Receiver<MessageForOscThread>, timer: Arc<Mutex<Instant>>) {
    let sock = UdpSocket::bind(osc_settings.host_addr).unwrap();

    let mut beat: Vec<Beat> = (0..BEAT_COUNT).map(|_| Beat::new()).collect();
    let mut master_velocity = 1.0;

    loop {
        // Handle events
        while let Ok(msg) = rx.try_recv() {
            match msg {
                MessageForOscThread::Terminate => return,
                MessageForOscThread::UpdateBeat(beat_index, new_freq, new_phase) => {
                    beat[beat_index].frequency = new_freq;
                    beat[beat_index].phase = new_phase;
                },
                MessageForOscThread::UpdateVelocity(beat_index, new_velocity) => {
                    beat[beat_index].velocity = new_velocity;
                },
                MessageForOscThread::UpdateMasterVelocity(new_velocity) => {
                    master_velocity = new_velocity;
                },
            }
        }

        // Send OSC messages
        //  - Send beats
        for i in 0..BEAT_COUNT {
            let value = beat[i].eval(&timer);
            let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
                addr: format!("/beat{}", i+1).to_string(),
                args: Some(vec![
                    OscType::Float(value),
                    OscType::Float(beat[i].frequency),
                    OscType::Float(beat[i].phase),
                    OscType::Float(beat[i].velocity * master_velocity)
                ]),
            })).unwrap();
            sock.send_to(&msg_buf, osc_settings.to_addr).unwrap();
        }

        //  - Send time
        let t = {
            let timer = timer.lock().unwrap();
            timer.elapsed().as_millis() as f32 / 1000.0
        };
        let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/time".to_string(),
            args: Some(vec![OscType::Float(t)]),
        })).unwrap();
        sock.send_to(&msg_buf, osc_settings.to_addr).unwrap();

        thread::sleep(Duration::from_millis(SLEEP_TIME));
    }
}

fn get_addr_from_arg(arg: &str) -> SocketAddrV4 {
    SocketAddrV4::from_str(arg).unwrap()
}


