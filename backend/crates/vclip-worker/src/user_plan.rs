//! User plan utilities for the worker.
//!
//! Provides shared functionality for resolving user plan tiers from Firestore.
//! This module follows the DRY principle by centralizing plan resolution logic
//! used by quota enforcement, watermarking, and other plan-dependent features.

use tracing::{debug, info, warn};
use vclip_firestore::{FirestoreClient, FromFirestoreValue};
use vclip_models::PlanTier;

/// Resolved user plan with associated features and limits.
#[derive(Debug, Clone)]
pub struct UserPlan {
    /// The user's plan tier
    pub tier: PlanTier,
    /// Storage limit in bytes for this plan
    pub storage_limit_bytes: u64,
    /// Whether exports require watermark
    pub requires_watermark: bool,
}

impl Default for UserPlan {
    fn default() -> Self {
        Self::for_tier(PlanTier::Free)
    }
}

impl UserPlan {
    /// Create a UserPlan for a specific tier.
    pub fn for_tier(tier: PlanTier) -> Self {
        Self {
            tier,
            storage_limit_bytes: tier.storage_limit_bytes(),
            requires_watermark: tier.requires_watermark(),
        }
    }
}

/// Resolve user plan tier from Firestore.
///
/// # Caching
/// This function does NOT cache results. For repeated lookups in the same
/// request, callers should cache the result themselves.
///
/// # Fallback Behavior
/// Returns `PlanTier::Free` if:
/// - User document doesn't exist
/// - User document has no plan field
/// - Firestore lookup fails
///
/// This fail-safe behavior ensures plan-dependent features (like watermarking)
/// default to the most restrictive tier on errors.
pub async fn resolve_user_plan(
    firestore: &FirestoreClient,
    user_id: &str,
) -> UserPlan {
    let tier = resolve_user_tier(firestore, user_id).await;
    UserPlan::for_tier(tier)
}

/// Resolve just the plan tier from Firestore.
///
/// Lower-level function when only the tier is needed.
pub async fn resolve_user_tier(
    firestore: &FirestoreClient,
    user_id: &str,
) -> PlanTier {
    match firestore.get_document("users", user_id).await {
        Ok(Some(doc)) => {
            if let Some(fields) = doc.fields {
                // Check plan field (try both "plan_tier" and "plan" for backwards compat)
                let plan_str = fields
                    .get("plan_tier")
                    .and_then(|v| String::from_firestore_value(v))
                    .or_else(|| {
                        fields
                            .get("plan")
                            .and_then(|v| String::from_firestore_value(v))
                    });

                if let Some(plan) = plan_str {
                    let tier = PlanTier::from_str(&plan);
                    debug!(
                        user_id = %user_id,
                        plan = %plan,
                        tier = ?tier,
                        "Resolved user plan tier"
                    );
                    return tier;
                }
            }
            
            // No plan field found - default to Free
            debug!(
                user_id = %user_id,
                "No plan field in user document, defaulting to Free"
            );
            PlanTier::Free
        }
        Ok(None) => {
            // User document not found - default to Free
            debug!(
                user_id = %user_id,
                "User document not found, defaulting to Free"
            );
            PlanTier::Free
        }
        Err(e) => {
            // Firestore error - default to Free (fail-safe)
            warn!(
                user_id = %user_id,
                error = %e,
                "Failed to fetch user document, defaulting to Free"
            );
            PlanTier::Free
        }
    }
}

/// Check if user requires watermark on exports (convenience function).
///
/// Returns `true` if user is on Free plan, `false` for Pro/Studio.
pub async fn user_requires_watermark(
    firestore: &FirestoreClient,
    user_id: &str,
) -> bool {
    let plan = resolve_user_plan(firestore, user_id).await;
    let requires = plan.requires_watermark;
    
    if requires {
        info!(
            user_id = %user_id,
            tier = ?plan.tier,
            "User requires watermark (free tier)"
        );
    }
    
    requires
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_plan_for_tier() {
        let free = UserPlan::for_tier(PlanTier::Free);
        assert_eq!(free.tier, PlanTier::Free);
        assert!(free.requires_watermark);
        
        let pro = UserPlan::for_tier(PlanTier::Pro);
        assert_eq!(pro.tier, PlanTier::Pro);
        assert!(!pro.requires_watermark);
        
        let studio = UserPlan::for_tier(PlanTier::Studio);
        assert_eq!(studio.tier, PlanTier::Studio);
        assert!(!studio.requires_watermark);
    }

    #[test]
    fn test_user_plan_storage_limits() {
        let free = UserPlan::for_tier(PlanTier::Free);
        assert_eq!(free.storage_limit_bytes, 1024 * 1024 * 1024); // 1 GB
        
        let pro = UserPlan::for_tier(PlanTier::Pro);
        assert_eq!(pro.storage_limit_bytes, 30 * 1024 * 1024 * 1024); // 30 GB
    }

    #[test]
    fn test_default_is_free() {
        let default = UserPlan::default();
        assert_eq!(default.tier, PlanTier::Free);
        assert!(default.requires_watermark);
    }
}
