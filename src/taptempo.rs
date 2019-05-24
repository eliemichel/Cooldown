use std::iter::Iterator;

pub struct TapTempo {
	pub frequency: f32,
	pub phase: f32,

	accum: Vec<f32>,
}

impl TapTempo {
	pub fn new() -> TapTempo {
		TapTempo{ frequency: 0.0, phase: 0.0, accum: vec![] }
	}

	pub fn sample_count(&self) -> usize {
		self.accum.len()
	}

	pub fn reset(&mut self) {
        println!("Resetting estimator");
        self.accum.clear();
    }

    pub fn add_sample(&mut self, value: f32) {
        self.accum.push(value);
    }

	pub fn estimate(&mut self)
    {
    	if self.accum.len() < 2 {
    		return;
    	}
        let count = self.accum.len();
        let first_tap = self.accum[0];
        let last_tap = self.accum[count - 1];
        let period = (last_tap - first_tap) / (count - 1) as f32;
        self.phase = 0.0;
        for (i, v) in self.accum.iter().enumerate() {
            self.phase += v - (i as f32) * period;
        }
        self.phase /= count as f32;
        self.frequency = 1.0 / period;
    }
}
