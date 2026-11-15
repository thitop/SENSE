use std::time::Instant;

pub struct Timing {
    last_frame_time: Instant,
    delta_time: f32,
    fps_timer: Instant,
    frame_count: u32,
    current_fps: u32,
    accumulator: f32,
    pub fixed_time_step: f32, // e.g. 1.0 / 1000.0 (1000 Hz physics/tick)
}

impl Timing {
    pub fn new(fixed_hz: f32) -> Self {
        let now = Instant::now();
        Self {
            last_frame_time: now,
            delta_time: 0.0,
            fps_timer: now,
            frame_count: 0,
            current_fps: 0,
            accumulator: 0.0,
            fixed_time_step: 1.0 / fixed_hz.max(1.0),
        }
    }

    pub fn update(&mut self) -> f32 {
        let now = Instant::now();
        let frame_duration = now.duration_since(self.last_frame_time);
        self.last_frame_time = now;

        self.delta_time = frame_duration.as_secs_f32().min(0.1); // clamp to prevent spiral of death
        self.accumulator += self.delta_time;

        self.frame_count += 1;
        if now.duration_since(self.fps_timer).as_secs_f32() >= 1.0 {
            self.current_fps = self.frame_count;
            self.frame_count = 0;
            self.fps_timer = now;
        }

        self.delta_time
    }

    pub fn should_do_fixed_update(&mut self) -> bool {
        if self.accumulator >= self.fixed_time_step {
            self.accumulator -= self.fixed_time_step;
            true
        } else {
            false
        }
    }

    pub fn delta_time(&self) -> f32 {
        self.delta_time
    }

    pub fn fps(&self) -> u32 {
        self.current_fps
    }
}

