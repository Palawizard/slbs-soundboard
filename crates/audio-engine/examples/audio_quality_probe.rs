use std::time::Duration;

use slb_audio_engine::run_reference_quality_probe;

fn main() {
    let report = run_reference_quality_probe(Duration::from_secs(10));
    println!(
        "resampler_latency_frames={}",
        report.resampler_latency_frames
    );
    println!("tone_gain_db={:.6}", report.tone_gain_db);
    println!("frequency_error_hz={:.6}", report.frequency_error_hz);
    println!("clipped_samples={}", report.clipped_samples);
    println!("dropout_frames={}", report.dropout_frames);
    println!("simulated_frames={}", report.simulated_frames);
    if !report.passes() {
        std::process::exit(1);
    }
}
