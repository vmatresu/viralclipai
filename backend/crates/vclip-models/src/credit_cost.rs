//! Credit cost calculation utilities.
//!
//! This module provides reusable cost calculation logic for credit-based operations.
//! It follows the builder pattern for flexible configuration and returns a structured
//! breakdown for use in transaction recording.
//!
//! # Example
//!
//! ```ignore
//! use vclip_models::{Style, ReprocessingCostCalculator};
//!
//! let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit, Style::Original], 2)
//!     .with_silent_remover(true)
//!     .with_object_detection(false)
//!     .calculate();
//!
//! assert_eq!(cost.total, 50); // (10 + 10) * 2 scenes + 5 * 2 scenes
//! ```

use std::collections::HashMap;

use crate::{Style, SILENT_REMOVER_ADDON_COST, OBJECT_DETECTION_ADDON_COST};

// =============================================================================
// Cost Breakdown
// =============================================================================

/// Detailed breakdown of credit costs for a reprocessing operation.
///
/// This struct provides all the information needed for:
/// - Displaying cost to users before processing
/// - Recording transaction metadata for credit history
/// - Generating human-readable descriptions
#[derive(Debug, Clone)]
pub struct CostBreakdown {
    /// Per-style costs: (style_name, total_cost_for_style)
    pub style_costs: Vec<(String, u32)>,
    /// Total credits for all styles
    pub style_total: u32,
    /// Credits for silent remover addon (0 if disabled)
    pub silent_remover_cost: u32,
    /// Credits for object detection addon (0 if disabled)
    pub object_detection_cost: u32,
    /// Grand total of all credits
    pub total: u32,
    /// Number of scenes being processed
    pub scene_count: u32,
}

impl CostBreakdown {
    /// Generate a human-readable description for credit transactions.
    ///
    /// Format: "Reprocess N scene(s) (style1, style2) + addon1 + addon2"
    pub fn to_description(&self) -> String {
        let styles: Vec<&str> = self.style_costs.iter().map(|(s, _)| s.as_str()).collect();
        
        let mut addons: Vec<&str> = Vec::new();
        if self.silent_remover_cost > 0 {
            addons.push("silent remover");
        }
        if self.object_detection_cost > 0 {
            addons.push("object detection");
        }
        
        let scene_text = if self.scene_count == 1 { "scene" } else { "scenes" };
        
        if addons.is_empty() {
            format!(
                "Reprocess {} {} ({})",
                self.scene_count,
                scene_text,
                styles.join(", ")
            )
        } else {
            format!(
                "Reprocess {} {} ({}) + {}",
                self.scene_count,
                scene_text,
                styles.join(", "),
                addons.join(" + ")
            )
        }
    }
    
    /// Convert to metadata HashMap for transaction recording.
    ///
    /// Keys produced:
    /// - `scene_count`: Number of scenes
    /// - `style_breakdown`: Per-style costs (format: "style1:cost1,style2:cost2")
    /// - `style_credits`: Total style credits
    /// - `silent_remover`: "true" (only if enabled)
    /// - `silent_remover_credits`: Addon cost (only if enabled)
    /// - `object_detection`: "true" (only if enabled)
    /// - `object_detection_credits`: Addon cost (only if enabled)
    /// - `total_credits`: Grand total
    pub fn to_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        
        metadata.insert("scene_count".to_string(), self.scene_count.to_string());
        
        // Per-style breakdown: "streamer_split:10,original:10"
        let style_breakdown: Vec<String> = self.style_costs
            .iter()
            .map(|(name, cost)| format!("{}:{}", name, cost))
            .collect();
        metadata.insert("style_breakdown".to_string(), style_breakdown.join(","));
        metadata.insert("style_credits".to_string(), self.style_total.to_string());
        
        if self.silent_remover_cost > 0 {
            metadata.insert("silent_remover".to_string(), "true".to_string());
            metadata.insert("silent_remover_credits".to_string(), self.silent_remover_cost.to_string());
        }
        
        if self.object_detection_cost > 0 {
            metadata.insert("object_detection".to_string(), "true".to_string());
            metadata.insert("object_detection_credits".to_string(), self.object_detection_cost.to_string());
        }
        
        metadata.insert("total_credits".to_string(), self.total.to_string());
        
        metadata
    }
}

// =============================================================================
// Cost Calculator
// =============================================================================

/// Builder for calculating reprocessing costs.
///
/// Follows the builder pattern for flexible configuration.
/// Calculates costs based on styles, scene count, and addons.
#[derive(Debug, Clone)]
pub struct ReprocessingCostCalculator {
    styles: Vec<Style>,
    num_scenes: u32,
    cut_silent_parts: bool,
    enable_object_detection: bool,
}

impl ReprocessingCostCalculator {
    /// Create a new calculator with required parameters.
    ///
    /// # Arguments
    /// * `styles` - Styles to apply (each has its own credit cost)
    /// * `num_scenes` - Number of scenes to process
    pub fn new(styles: Vec<Style>, num_scenes: u32) -> Self {
        Self {
            styles,
            num_scenes,
            cut_silent_parts: false,
            enable_object_detection: false,
        }
    }
    
    /// Enable silent remover addon.
    ///
    /// Adds `SILENT_REMOVER_ADDON_COST` credits per scene.
    pub fn with_silent_remover(mut self, enabled: bool) -> Self {
        self.cut_silent_parts = enabled;
        self
    }
    
    /// Enable object detection addon.
    ///
    /// Adds `OBJECT_DETECTION_ADDON_COST` credits per scene.
    pub fn with_object_detection(mut self, enabled: bool) -> Self {
        self.enable_object_detection = enabled;
        self
    }
    
    /// Calculate the total cost breakdown.
    ///
    /// Returns a `CostBreakdown` with all costs itemized.
    pub fn calculate(&self) -> CostBreakdown {
        // Calculate per-style costs
        let style_costs: Vec<(String, u32)> = self.styles
            .iter()
            .map(|s| (s.as_filename_part().to_string(), s.credit_cost() * self.num_scenes))
            .collect();
        
        let style_total: u32 = style_costs.iter().map(|(_, cost)| cost).sum();
        
        // Calculate addon costs
        let silent_remover_cost = if self.cut_silent_parts {
            SILENT_REMOVER_ADDON_COST * self.num_scenes
        } else {
            0
        };
        
        let object_detection_cost = if self.enable_object_detection {
            OBJECT_DETECTION_ADDON_COST * self.num_scenes
        } else {
            0
        };
        
        let total = style_total + silent_remover_cost + object_detection_cost;
        
        CostBreakdown {
            style_costs,
            style_total,
            silent_remover_cost,
            object_detection_cost,
            total,
            scene_count: self.num_scenes,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_style_no_addons() {
        let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit], 1)
            .calculate();
        
        assert_eq!(cost.style_costs.len(), 1);
        assert_eq!(cost.style_costs[0], ("streamer_split".to_string(), 10));
        assert_eq!(cost.style_total, 10);
        assert_eq!(cost.silent_remover_cost, 0);
        assert_eq!(cost.object_detection_cost, 0);
        assert_eq!(cost.total, 10);
    }
    
    #[test]
    fn test_multiple_styles() {
        let cost = ReprocessingCostCalculator::new(
            vec![Style::StreamerSplit, Style::Original],
            2
        ).calculate();
        
        assert_eq!(cost.style_costs.len(), 2);
        assert_eq!(cost.style_total, 30); // (10 + 5) * 2 scenes
        assert_eq!(cost.total, 30);
    }
    
    #[test]
    fn test_with_silent_remover() {
        let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit], 2)
            .with_silent_remover(true)
            .calculate();
        
        assert_eq!(cost.style_total, 20); // 10 * 2 scenes
        assert_eq!(cost.silent_remover_cost, 10); // 5 * 2 scenes
        assert_eq!(cost.total, 30);
    }
    
    #[test]
    fn test_with_object_detection() {
        let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit], 1)
            .with_object_detection(true)
            .calculate();
        
        assert_eq!(cost.style_total, 10);
        assert_eq!(cost.object_detection_cost, 10); // 10 * 1 scene
        assert_eq!(cost.total, 20);
    }
    
    #[test]
    fn test_all_addons() {
        let cost = ReprocessingCostCalculator::new(
            vec![Style::StreamerSplit, Style::Original],
            1
        )
        .with_silent_remover(true)
        .with_object_detection(true)
        .calculate();
        
        assert_eq!(cost.style_total, 15); // 10 + 5
        assert_eq!(cost.silent_remover_cost, 5); // 5 * 1
        assert_eq!(cost.object_detection_cost, 10); // 10 * 1
        assert_eq!(cost.total, 30);
    }
    
    #[test]
    fn test_description_no_addons() {
        let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit], 1)
            .calculate();
        
        assert_eq!(
            cost.to_description(),
            "Reprocess 1 scene (streamer_split)"
        );
    }
    
    #[test]
    fn test_description_with_addons() {
        let cost = ReprocessingCostCalculator::new(
            vec![Style::StreamerSplit, Style::Original],
            2
        )
        .with_silent_remover(true)
        .with_object_detection(true)
        .calculate();
        
        assert_eq!(
            cost.to_description(),
            "Reprocess 2 scenes (streamer_split, original) + silent remover + object detection"
        );
    }
    
    #[test]
    fn test_metadata_generation() {
        let cost = ReprocessingCostCalculator::new(vec![Style::StreamerSplit], 1)
            .with_silent_remover(true)
            .calculate();
        
        let metadata = cost.to_metadata();
        
        assert_eq!(metadata.get("scene_count"), Some(&"1".to_string()));
        assert_eq!(metadata.get("style_breakdown"), Some(&"streamer_split:10".to_string()));
        assert_eq!(metadata.get("style_credits"), Some(&"10".to_string()));
        assert_eq!(metadata.get("silent_remover"), Some(&"true".to_string()));
        assert_eq!(metadata.get("silent_remover_credits"), Some(&"5".to_string()));
        assert_eq!(metadata.get("total_credits"), Some(&"15".to_string()));
        // Object detection should not be present
        assert!(metadata.get("object_detection").is_none());
    }
}
