//! Layout behavior and presentation for the prompt composer.

use super::{
    APP_SIDEBAR_WIDTH, HOME_COMPOSER_MAX_WIDTH, MODEL_PICKER_MIN_SUBMENU_WIDTH,
    MODEL_PICKER_RIGHT_INSET, MODEL_PICKER_SUBMENU_GAP, PARTICLE_TIMELINE_MS, SubmenuLayout,
};

pub(super) fn submenu_layout(viewport_width: f32, natural_width: f32) -> SubmenuLayout {
    let main_width = (viewport_width - APP_SIDEBAR_WIDTH).max(0.0);
    let composer_width = main_width.min(HOME_COMPOSER_MAX_WIDTH);
    let trailing_margin = ((main_width - composer_width) * 0.5).max(0.0);
    submenu_layout_at_right(trailing_margin, natural_width)
}

pub(super) fn submenu_layout_at_right(trailing_margin: f32, natural_width: f32) -> SubmenuLayout {
    let available_right = trailing_margin + MODEL_PICKER_RIGHT_INSET - MODEL_PICKER_SUBMENU_GAP;

    if available_right >= MODEL_PICKER_MIN_SUBMENU_WIDTH {
        SubmenuLayout {
            open_left: false,
            width: natural_width.min(available_right),
        }
    } else {
        SubmenuLayout {
            open_left: true,
            width: natural_width,
        }
    }
}

pub(super) fn particle_transition_ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // Invert x for the reference cubic-bezier(.45, 0, .55, 1), then evaluate y.
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * 0.45
            + 3.0 * inverse * parameter * parameter * 0.55
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * parameter * parameter + parameter * parameter * parameter
}

pub(super) fn particle_noise(index: usize, step: u32, channel: u32) -> f32 {
    let mut value = (index as u32 + 1)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(step.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(channel.wrapping_mul(0xc2b2_ae35));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

pub(super) fn max_particle_drift(progress: f32, index: usize, duration_ms: u64) -> (f32, f32) {
    let segment_count = (PARTICLE_TIMELINE_MS / duration_ms as f32).round() as u32;
    let local = progress * segment_count as f32 + index as f32 * 0.37;
    let segment = local.floor() as u32 % segment_count;
    let next_segment = (segment + 1) % segment_count;
    let eased = particle_transition_ease(local.fract());
    let interpolate = |from: f32, to: f32| from + (to - from) * eased;

    let x = interpolate(
        particle_noise(index, segment, 0),
        particle_noise(index, next_segment, 0),
    );
    let y = interpolate(
        particle_noise(index, segment, 1),
        particle_noise(index, next_segment, 1),
    );
    ((x - 0.5) * 6.0, (y - 0.5) * 8.0)
}

pub(super) fn particle_layers(ultra_mode: bool, accelerated: bool) -> (bool, bool) {
    let show_fast_particles = accelerated;
    // CDP: at data-max=true + data-fast-mode=true, MaxEffects retains only
    // its gradient canvas; the drifting TrackParticles layer is unmounted.
    let show_max_particles = ultra_mode && !accelerated;
    (show_max_particles, show_fast_particles)
}
