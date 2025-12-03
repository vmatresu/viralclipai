#!/usr/bin/env python3
"""
Migration Script: Initialize Plans in Firestore

This script migrates hardcoded plans to Firestore and ensures
default plans exist in the database.

Usage:
    python scripts/migrate_plans.py
"""

import os
import sys
from pathlib import Path

# Add parent directory to path to import app modules
sys.path.insert(0, str(Path(__file__).parent.parent))

from app.core.plans.service import PlanService
from app.core.plans.models import Plan, PlanStatus
from app.config import logger


def main():
    """Run plan migration."""
    logger.info("Starting plan migration...")
    
    plan_service = PlanService()
    
    try:
        # Ensure default plans exist
        plan_service.ensure_default_plans_exist()
        logger.info("Default plans ensured")
        
        # List all plans
        plans = plan_service.get_all_plans(include_inactive=True)
        logger.info("Found %d plans in Firestore:", len(plans))
        
        for plan in plans:
            logger.info(
                "  - %s (%s): %s clips/month, $%s/month, status=%s",
                plan.id,
                plan.name,
                plan.get_limit("max_clips_per_month", 0),
                plan.price_monthly or "custom",
                plan.status.value
            )
        
        logger.info("Plan migration completed successfully")
        return 0
    
    except Exception as e:
        logger.error("Plan migration failed: %s", e, exc_info=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())

