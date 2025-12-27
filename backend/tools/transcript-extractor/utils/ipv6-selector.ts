/**
 * IPv6 Address Selector
 *
 * Provides random IPv6 address selection for rate limiting avoidance.
 * Used by multiple strategies (youtubei, yt-dlp) to rotate source addresses.
 *
 * @module utils/ipv6-selector
 */

import os from "node:os";

/**
 * IPv6 address information for logging
 */
export interface IPv6SelectionResult {
  address: string | null;
  availableCount: number;
  isRotated: boolean;
}

/**
 * Get all global (routable) IPv6 addresses from network interfaces
 *
 * Filters out:
 * - Link-local addresses (fe80::)
 * - Loopback (::1)
 * - Unique Local Addresses (fc00::/fd00::)
 * - Internal interfaces
 */
export function getGlobalIPv6Addresses(): string[] {
  try {
    const interfaces = os.networkInterfaces();
    const globalAddresses: string[] = [];

    for (const [, addrs] of Object.entries(interfaces)) {
      if (!addrs) continue;

      for (const addr of addrs) {
        // Skip non-IPv6
        if (addr.family !== "IPv6") continue;
        // Skip internal/loopback
        if (addr.internal) continue;
        // Skip link-local (fe80::)
        if (addr.address.startsWith("fe80:")) continue;
        // Skip loopback
        if (addr.address === "::1") continue;
        // Skip Unique Local Addresses (ULA)
        if (addr.address.startsWith("fc") || addr.address.startsWith("fd")) {
          continue;
        }

        globalAddresses.push(addr.address);
      }
    }

    return globalAddresses;
  } catch {
    return [];
  }
}

/**
 * Select a random global IPv6 address for source binding
 *
 * @returns Random IPv6 address or null if none available
 */
export function selectRandomIPv6Address(): string | null {
  const addresses = getGlobalIPv6Addresses();

  if (addresses.length === 0) {
    return null;
  }

  const randomIndex = Math.floor(Math.random() * addresses.length);
  return addresses[randomIndex];
}

/**
 * Select a random IPv6 address with metadata for logging
 *
 * @returns Selection result with address and metadata
 */
export function selectIPv6WithMetadata(): IPv6SelectionResult {
  const addresses = getGlobalIPv6Addresses();

  if (addresses.length === 0) {
    return {
      address: null,
      availableCount: 0,
      isRotated: false,
    };
  }

  const randomIndex = Math.floor(Math.random() * addresses.length);
  return {
    address: addresses[randomIndex],
    availableCount: addresses.length,
    isRotated: addresses.length > 1,
  };
}

/**
 * Check if IPv6 rotation is available
 *
 * @returns True if multiple global IPv6 addresses are available
 */
export function isIPv6RotationAvailable(): boolean {
  return getGlobalIPv6Addresses().length > 1;
}
