// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

pub mod bitmap;
pub mod convert;
pub mod curve;
pub mod math_utils;
pub mod polygon;
pub mod types;
