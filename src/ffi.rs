use std::sync::Mutex;

use crate::sm::Note;

#[repr(C)]
struct MinaNoteC {
    notes: u32,
    row_time: f32,
}

extern "C" {
    fn mina_calc_version() -> i32;
    fn mina_sd_all_rates(
        notes: *const MinaNoteC,
        count: i32,
        keycount: u32,
        out: *mut f32,
    ) -> i32;
}

static CALC: Mutex<()> = Mutex::new(());

pub fn calc_version() -> i32 {
    unsafe { mina_calc_version() }
}

/// 14 rates, 0.70 through 2.00, each with 8 skillset values.
pub fn rate_notes(notes: &[Note], key_count: u32) -> Result<[[f32; 8]; 14], String> {
    let _guard = CALC.lock().unwrap_or_else(|poison| poison.into_inner());
    let rows: Vec<MinaNoteC> = notes
        .iter()
        .map(|note| MinaNoteC {
            notes: note.mask,
            row_time: note.time,
        })
        .collect();
    let mut flat = [0.0f32; 14 * 8];
    let code = unsafe {
        mina_sd_all_rates(
            rows.as_ptr(),
            rows.len() as i32,
            key_count,
            flat.as_mut_ptr(),
        )
    };
    if code != 0 {
        return Err("计算器中途失败了".to_owned());
    }
    let mut ratings = [[0.0f32; 8]; 14];
    for rate in 0..14 {
        ratings[rate].copy_from_slice(&flat[rate * 8..rate * 8 + 8]);
    }
    Ok(ratings)
}
