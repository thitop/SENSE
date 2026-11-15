use super::path_recorder::ShotMovementPath;

#[derive(Clone, Debug)]
pub struct SensPerformanceSample {
    pub cm_360: f64,
    pub reaction_time_ms: f64,
    pub accuracy: f64,
    pub path_efficiency: f64,
    pub composite_score: f64,
}

#[derive(Clone, Debug)]
pub struct StatisticalAnalyzer {
    pub samples: Vec<SensPerformanceSample>,
}

impl StatisticalAnalyzer {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(256),
        }
    }

    /// Record a shot at a specific cm/360° sensitivity
    pub fn record_sample(
        &mut self,
        cm_360: f64,
        accuracy: f64,
        reaction_time_ms: f64,
        path: &ShotMovementPath,
    ) {
        let net_disp = (path.samples.last().map(|s| s.accumulated_pos.length()).unwrap_or(1.0)) as f64;
        let efficiency = (net_disp / path.total_mouse_distance.max(1.0) as f64).clamp(0.1, 1.0);

        // Composite Performance Function:
        // Score = 0.35 * Accuracy + 0.30 * SpeedFactor + 0.25 * PathEfficiency + 0.10 * Consistency
        let speed_factor = (1200.0 / reaction_time_ms.max(150.0)).clamp(0.2, 2.5) * 40.0;
        let composite = (accuracy * 0.35) + (speed_factor * 0.30) + (efficiency * 100.0 * 0.35);

        self.samples.push(SensPerformanceSample {
            cm_360,
            reaction_time_ms,
            accuracy,
            path_efficiency: efficiency,
            composite_score: composite,
        });
    }

    /// Fit a 2nd-degree polynomial curve: Score(x) = a*x^2 + b*x + c
    /// Where x is cm/360°, and find the vertex peak where d/dx = 0 (x = -b / 2a)
    pub fn find_optimal_cm_360(&self) -> Option<f64> {
        if self.samples.len() < 5 {
            return None;
        }

        // Polynomial regression (degree 2) using least squares:
        // Solving normal equations: (X^T * X) * [c, b, a]^T = X^T * Y
        let n = self.samples.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_x2 = 0.0;
        let mut sum_x3 = 0.0;
        let mut sum_x4 = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2y = 0.0;

        for s in &self.samples {
            let x = s.cm_360;
            let y = s.composite_score;
            let x2 = x * x;
            sum_x += x;
            sum_x2 += x2;
            sum_x3 += x2 * x;
            sum_x4 += x2 * x2;
            sum_y += y;
            sum_xy += x * y;
            sum_x2y += x2 * y;
        }

        // 3x3 matrix determinant method (Cramer's rule)
        let det = n * (sum_x2 * sum_x4 - sum_x3 * sum_x3)
            - sum_x * (sum_x * sum_x4 - sum_x2 * sum_x3)
            + sum_x2 * (sum_x * sum_x3 - sum_x2 * sum_x2);

        if det.abs() < 1e-7 {
            return None;
        }

        let det_a = n * (sum_x2 * sum_x2y - sum_x3 * sum_xy)
            - sum_x * (sum_x * sum_x2y - sum_x2 * sum_xy)
            + sum_y * (sum_x * sum_x3 - sum_x2 * sum_x2);

        let det_b = n * (sum_xy * sum_x4 - sum_x2y * sum_x3)
            - sum_y * (sum_x * sum_x4 - sum_x2 * sum_x3)
            + sum_x2 * (sum_x * sum_x2y - sum_xy * sum_x2);

        let a = det_a / det;
        let b = det_b / det;

        // If a < 0, the parabola opens downward and has a unique maximum peak!
        if a < 0.0 {
            let peak_x = -b / (2.0 * a);
            // Clamp within reasonable physical limits [10 cm/360°, 100 cm/360°]
            Some(peak_x.clamp(12.0, 95.0))
        } else {
            // Parabola opened upwards or flat, pick sample with highest composite score
            self.samples
                .iter()
                .max_by(|s1, s2| s1.composite_score.partial_cmp(&s2.composite_score).unwrap())
                .map(|s| s.cm_360)
        }
    }
}

