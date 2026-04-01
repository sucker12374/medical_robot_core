use std::collections::HashMap;
use std::sync::Mutex;

use crate::types::{RobotId, ZoneId};

/// OS concept shown here: mutual exclusion for a shared hospital zone.
///
/// A zone can hold at most one robot id at a time. The mutex makes the check
/// and update happen as one safe step.
#[derive(Debug)]
pub struct ZoneManager {
    occupancy: Mutex<HashMap<ZoneId, Option<RobotId>>>,
}

impl ZoneManager {
    pub fn new(zones: impl IntoIterator<Item = ZoneId>) -> Self {
        let occupancy = zones.into_iter().map(|zone| (zone, None)).collect();
        Self {
            occupancy: Mutex::new(occupancy),
        }
    }

    pub fn try_acquire(&self, zone_id: &str, robot_id: RobotId) -> bool {
        let mut occupancy = self.occupancy.lock().unwrap();
        match occupancy.get_mut(zone_id) {
            Some(slot @ None) => {
                // The zone was free, so this robot becomes the only owner.
                *slot = Some(robot_id);
                true
            }
            // The zone is busy, so the caller must retry later.
            Some(Some(_)) => false,
            None => false,
        }
    }

    pub fn release(&self, zone_id: &str, robot_id: RobotId) -> Result<(), ZoneReleaseError> {
        let mut occupancy = self.occupancy.lock().unwrap();
        match occupancy.get_mut(zone_id) {
            Some(slot) => match *slot {
                Some(owner) if owner == robot_id => {
                    // Give the zone back so another robot may enter.
                    *slot = None;
                    Ok(())
                }
                Some(owner) => Err(ZoneReleaseError::WrongOwner {
                    zone_id: zone_id.to_string(),
                    expected_owner: owner,
                    attempted_by: robot_id,
                }),
                None => Err(ZoneReleaseError::ZoneNotOccupied(zone_id.to_string())),
            },
            None => Err(ZoneReleaseError::UnknownZone(zone_id.to_string())),
        }
    }

    pub fn occupant(&self, zone_id: &str) -> Option<RobotId> {
        self.occupancy
            .lock()
            .unwrap()
            .get(zone_id)
            .copied()
            .flatten()
    }

    pub fn snapshot(&self) -> HashMap<ZoneId, Option<RobotId>> {
        self.occupancy.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneReleaseError {
    UnknownZone(ZoneId),
    ZoneNotOccupied(ZoneId),
    WrongOwner {
        zone_id: ZoneId,
        expected_owner: RobotId,
        attempted_by: RobotId,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::{ZoneManager, ZoneReleaseError};

    #[test]
    fn acquire_succeeds_on_free_zone() {
        let manager = ZoneManager::new(["Zone-A".to_string()]);

        assert!(manager.try_acquire("Zone-A", 1));
        assert_eq!(manager.occupant("Zone-A"), Some(1));
    }

    #[test]
    fn acquire_fails_on_occupied_zone() {
        let manager = ZoneManager::new(["Zone-A".to_string()]);

        assert!(manager.try_acquire("Zone-A", 1));
        assert!(!manager.try_acquire("Zone-A", 2));
        assert_eq!(manager.occupant("Zone-A"), Some(1));
    }

    #[test]
    fn release_succeeds_for_current_owner() {
        let manager = ZoneManager::new(["Zone-A".to_string()]);

        assert!(manager.try_acquire("Zone-A", 4));
        assert_eq!(manager.release("Zone-A", 4), Ok(()));
        assert_eq!(manager.occupant("Zone-A"), None);
    }

    #[test]
    fn contention_preserves_mutual_exclusion() {
        let manager = Arc::new(ZoneManager::new(["Zone-A".to_string()]));
        let barrier = Arc::new(Barrier::new(3));

        let handles: Vec<_> = [10, 20]
            .into_iter()
            .map(|robot_id| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    manager.try_acquire("Zone-A", robot_id)
                })
            })
            .collect();

        barrier.wait();

        let results: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
        let success_count = results.into_iter().filter(|success| *success).count();

        assert_eq!(success_count, 1);
        assert!(matches!(manager.occupant("Zone-A"), Some(10) | Some(20)));
    }

    #[test]
    fn release_by_wrong_robot_is_rejected() {
        let manager = ZoneManager::new(["Zone-A".to_string()]);

        assert!(manager.try_acquire("Zone-A", 7));
        assert_eq!(
            manager.release("Zone-A", 9),
            Err(ZoneReleaseError::WrongOwner {
                zone_id: "Zone-A".to_string(),
                expected_owner: 7,
                attempted_by: 9,
            })
        );
    }
}
