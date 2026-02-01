use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Taic;

impl AtomWithProperties for Taic {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "TAIClockInfoBox",
            vec![
                (
                    "time_uncertainty",
                    AtomPropertyValue::from(self.time_uncertainty),
                ),
                (
                    "clock_resolution",
                    AtomPropertyValue::from(self.clock_resolution),
                ),
                (
                    "clock_drift_rate",
                    AtomPropertyValue::from(self.clock_drift_rate),
                ),
                (
                    "clock_type",
                    AtomPropertyValue::from(match self.clock_type {
                        mp4_atom::ClockType::Unknown => "Unknown",
                        mp4_atom::ClockType::DoesNotSync => "DoesNotSync",
                        mp4_atom::ClockType::CanSync => "CanSync",
                        mp4_atom::ClockType::Reserved => "Reserved",
                    }),
                ),
            ],
        )
    }
}
