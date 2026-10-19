//! The Richard Burns Rally 1.02 gameplay loop in Rust (milestone G, `notes/rust-plan.md`; design in
//! `lead/reports/gameplay-loop-rust.md`, the original's path in `notes/gameplay.md`): the input to the car
//! control, the frame clock and the substeps, the stage's states, timing and markers, the race flow, and the
//! championship around it (the season, the AI, the service park, the saves). `rbr-gameplay/README.md` (git 9c28c0b8) lists each module, the
//! original functions it ports and how it is checked.

pub mod ai_season;
pub mod camera_go;
pub mod car_damage;
pub mod championship;
pub mod control;
pub mod crt;
pub mod driveline;
pub mod ffb;
pub mod filter;
pub mod flow;
pub mod game;
pub mod ghost;
pub mod ini;
pub mod markers;
pub mod pause_menu;
pub mod race_clock;
pub mod race_options;
pub mod record;
pub mod result;
pub mod season;
pub mod season_events;
pub mod season_store;
pub mod service;
pub mod setup;
pub mod stage;
pub mod tick;
pub mod timing;
pub mod weather;
