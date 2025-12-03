"""
Saliency-based fallback for when face detection fails.

Provides simple, fast saliency estimation using edge density
and gradient magnitude.
"""

import cv2
import numpy as np

from app.core.smart_reframe.models import BoundingBox
from app.core.smart_reframe.config import FallbackPolicy


class SaliencyEstimator:
    """
    Estimate salient regions in a frame using fast heuristics.

    This is used as a fallback when face detection doesn't find
    any subjects.
    """

    def __init__(self, grid_size: int = 4):
        """
        Args:
            grid_size: Number of grid cells per dimension for saliency map.
        """
        self.grid_size = grid_size

    def get_focus_region(
        self,
        frame: np.ndarray,
        policy: FallbackPolicy = FallbackPolicy.UPPER_CENTER,
    ) -> BoundingBox:
        """
        Get the recommended focus region for a frame.

        Args:
            frame: BGR frame.
            policy: Fallback policy to use.

        Returns:
            BoundingBox indicating the focus region.
        """
        h, w = frame.shape[:2]

        if policy == FallbackPolicy.CENTER:
            return self._center_focus(w, h)
        elif policy == FallbackPolicy.UPPER_CENTER:
            return self._upper_center_focus(w, h)
        elif policy == FallbackPolicy.RULE_OF_THIRDS:
            return self._rule_of_thirds_focus(w, h)
        elif policy == FallbackPolicy.SALIENCY:
            return self._saliency_focus(frame)
        else:
            return self._center_focus(w, h)

    def _center_focus(self, width: int, height: int) -> BoundingBox:
        """Return center focus region."""
        focus_w = width * 0.6
        focus_h = height * 0.6
        return BoundingBox(
            x=(width - focus_w) / 2,
            y=(height - focus_h) / 2,
            width=focus_w,
            height=focus_h,
        )

    def _upper_center_focus(self, width: int, height: int) -> BoundingBox:
        """Return upper-center focus region (TikTok style)."""
        focus_w = width * 0.5
        focus_h = height * 0.5
        return BoundingBox(
            x=(width - focus_w) / 2,
            y=height * 0.15,  # Bias toward upper portion
            width=focus_w,
            height=focus_h,
        )

    def _rule_of_thirds_focus(self, width: int, height: int) -> BoundingBox:
        """Return focus on rule-of-thirds intersection."""
        # Focus on upper-third, center horizontally
        focus_w = width * 0.4
        focus_h = height * 0.4
        return BoundingBox(
            x=(width - focus_w) / 2,
            y=height / 3 - focus_h / 2,
            width=focus_w,
            height=focus_h,
        )

    def _saliency_focus(self, frame: np.ndarray) -> BoundingBox:
        """Compute saliency-based focus region."""
        h, w = frame.shape[:2]

        # Compute edge density
        gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
        edges = cv2.Canny(gray, 50, 150)

        # Compute saliency map on a grid
        cell_h = h // self.grid_size
        cell_w = w // self.grid_size

        saliency = np.zeros((self.grid_size, self.grid_size))

        for i in range(self.grid_size):
            for j in range(self.grid_size):
                y1 = i * cell_h
                y2 = (i + 1) * cell_h
                x1 = j * cell_w
                x2 = (j + 1) * cell_w

                cell = edges[y1:y2, x1:x2]
                saliency[i, j] = cell.sum()

        # Apply center bias (subjects more likely in center)
        center_bias = np.zeros((self.grid_size, self.grid_size))
        for i in range(self.grid_size):
            for j in range(self.grid_size):
                di = abs(i - self.grid_size / 2) / (self.grid_size / 2)
                dj = abs(j - self.grid_size / 2) / (self.grid_size / 2)
                center_bias[i, j] = 1 - 0.5 * (di + dj)

        # Apply upper bias (for TikTok-style content)
        upper_bias = np.zeros((self.grid_size, self.grid_size))
        for i in range(self.grid_size):
            upper_bias[i, :] = 1 - 0.3 * (i / self.grid_size)

        # Combine saliency with biases
        combined = saliency * center_bias * upper_bias

        # Normalize
        if combined.max() > 0:
            combined = combined / combined.max()

        # Find best region (using weighted centroid)
        total = combined.sum() + 1e-6
        cy = sum(i * combined[i, :].sum() for i in range(self.grid_size)) / total
        cx = sum(j * combined[:, j].sum() for j in range(self.grid_size)) / total

        # Convert grid coordinates to pixel coordinates
        focus_cx = (cx + 0.5) * cell_w
        focus_cy = (cy + 0.5) * cell_h

        # Create focus region
        focus_w = w * 0.5
        focus_h = h * 0.5

        return BoundingBox(
            x=focus_cx - focus_w / 2,
            y=focus_cy - focus_h / 2,
            width=focus_w,
            height=focus_h,
        ).clamp(w, h)

    def compute_saliency_map(self, frame: np.ndarray) -> np.ndarray:
        """
        Compute a saliency map for the frame.

        Args:
            frame: BGR frame.

        Returns:
            Normalized saliency map (0-1) at reduced resolution.
        """
        # Resize for speed
        small = cv2.resize(frame, (160, 90))

        # Convert to LAB for better color saliency
        lab = cv2.cvtColor(small, cv2.COLOR_BGR2LAB)

        # Compute gradients in each channel
        saliency = np.zeros(small.shape[:2], dtype=np.float32)

        for i in range(3):
            channel = lab[:, :, i].astype(np.float32)
            gx = cv2.Sobel(channel, cv2.CV_32F, 1, 0, ksize=3)
            gy = cv2.Sobel(channel, cv2.CV_32F, 0, 1, ksize=3)
            saliency += np.sqrt(gx**2 + gy**2)

        # Normalize
        if saliency.max() > 0:
            saliency = saliency / saliency.max()

        return saliency
