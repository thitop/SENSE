#[cfg(test)]
mod tests {
    use aim_trainer::game::hit_detection::{intersect_ray_sphere, Ray};
    use aim_trainer::sensitivity::converter::SensitivityConverter;
    use aim_trainer::sensitivity::games::GameProfile;
    use aim_trainer::sensitivity::path_recorder::ShotTrajectoryPattern;
    use aim_trainer::sensitivity::psa::PsaCalibrator;
    use glam::Vec3;

    #[test]
    fn test_sensitivity_conversion() {
        let valo = GameProfile {
            id: "valorant".into(),
            name: "Valorant".into(),
            yaw: 0.07,
            default_fov: 103.0,
        };

        // At 800 DPI and sens 0.35 in Valorant:
        // cm/360 = (360 * 2.54) / (800 * 0.35 * 0.07) = 914.4 / 19.6 ≈ 46.65 cm/360°
        let cm_360 = SensitivityConverter::game_sens_to_cm_360(&valo, 800.0, 0.35);
        assert!((cm_360 - 46.653).abs() < 0.05);

        // Convert back
        let back_sens = SensitivityConverter::cm_360_to_game_sens(&valo, 800.0, cm_360);
        assert!((back_sens - 0.35).abs() < 0.001);
    }

    #[test]
    fn test_ray_sphere_hit_detection() {
        let ray = Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };

        // Center on ray path at z = -10.0
        let target_center = Vec3::new(0.0, 1.5, -10.0);
        let target_radius = 0.35;

        let hit = intersect_ray_sphere(ray, target_center, target_radius);
        assert!(hit.is_some());
        let (dist, hit_pt, offset) = hit.unwrap();
        assert!((dist - 9.65).abs() < 0.01);
        assert!((hit_pt.z - (-9.65)).abs() < 0.001);
        assert!(offset < 0.001); // dead center
    }

    #[test]
    fn test_ray_sphere_miss() {
        let ray = Ray {
            origin: Vec3::new(0.0, 1.5, 0.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };

        // Target far off to the right
        let target_center = Vec3::new(5.0, 1.5, -10.0);
        let hit = intersect_ray_sphere(ray, target_center, 0.35);
        assert!(hit.is_none());
    }

    #[test]
    fn test_psa_binary_search_convergence() {
        let mut psa = PsaCalibrator::new(15.0, 80.0, 6);
        let initial_cm = psa.current_test_cm_360;
        assert_eq!(initial_cm, 47.5);

        // Simulate player overshooting (needs slower sens / higher cm)
        for _ in 0..10 {
            psa.record_shot(ShotTrajectoryPattern::Overshoot);
        }

        // Low bound should be updated to previous mid
        assert!(psa.current_test_cm_360 > 47.5);
    }
}
