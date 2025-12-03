/**
 * LocalStorage Storage Adapter
 *
 * Implements IStorageAdapter using browser localStorage.
 * Handles quota errors, serialization, and size estimation.
 */

import { frontendLogger } from "@/lib/logger";

import type { IStorageAdapter } from "../types";

/**
 * LocalStorage adapter implementation
 */
export class LocalStorageAdapter implements IStorageAdapter {
  private readonly prefix: string;
  private readonly logger = frontendLogger;

  constructor(prefix: string = "viralclipai_cache_") {
    this.prefix = prefix;
  }

  /**
   * Get a value from storage
   */
  async get<T>(key: string): Promise<T | null> {
    try {
      const prefixedKey = this.getPrefixedKey(key);
      const item = localStorage.getItem(prefixedKey);

      if (item === null) {
        return null;
      }

      return JSON.parse(item) as T;
    } catch (error) {
      this.logger.warn("Failed to get cache entry", { key, error });
      // Remove corrupted entry
      await this.remove(key).catch(() => {
        // Ignore removal errors
      });
      return null;
    }
  }

  /**
   * Set a value in storage
   */
  set<T>(key: string, value: T): Promise<void> {
    try {
      const prefixedKey = this.getPrefixedKey(key);
      const serialized = JSON.stringify(value);
      localStorage.setItem(prefixedKey, serialized);
      return Promise.resolve();
    } catch (error) {
      // Handle quota exceeded error
      if (this.isQuotaExceededError(error)) {
        this.logger.warn("Storage quota exceeded", { key });
        throw new Error("Storage quota exceeded");
      }
      throw error;
    }
  }

  /**
   * Remove a value from storage
   */
  remove(key: string): Promise<void> {
    try {
      const prefixedKey = this.getPrefixedKey(key);
      localStorage.removeItem(prefixedKey);
      return Promise.resolve();
    } catch (error) {
      this.logger.warn("Failed to remove cache entry", { key, error });
      throw error;
    }
  }

  /**
   * Clear all entries with the prefix
   */
  async clear(): Promise<void> {
    try {
      const keys = await this.keys();
      for (const key of keys) {
        localStorage.removeItem(key);
      }
    } catch (error) {
      this.logger.warn("Failed to clear cache", { error });
      throw error;
    }
  }

  /**
   * Get all keys with the prefix
   */
  keys(): Promise<string[]> {
    const keys: string[] = [];
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key?.startsWith(this.prefix)) {
          keys.push(key);
        }
      }
    } catch (error) {
      this.logger.warn("Failed to get cache keys", { error });
    }
    return Promise.resolve(keys);
  }

  /**
   * Estimate size of a stored value in bytes
   */
  size(key: string): Promise<number> {
    try {
      const prefixedKey = this.getPrefixedKey(key);
      const item = localStorage.getItem(prefixedKey);
      if (item === null) {
        return Promise.resolve(0);
      }
      // Estimate: UTF-16 encoding uses 2 bytes per character
      return Promise.resolve(item.length * 2);
    } catch (error) {
      this.logger.warn("Failed to get cache entry size", { key, error });
      return Promise.resolve(0);
    }
  }

  /**
   * Get prefixed key for namespacing
   */
  private getPrefixedKey(key: string): string {
    // Sanitize key to prevent XSS and ensure valid storage key
    const sanitized = this.sanitizeKey(key);
    return `${this.prefix}${sanitized}`;
  }

  /**
   * Sanitize key to prevent security issues
   */
  private sanitizeKey(key: string): string {
    // Remove any characters that could cause issues
    // Allow alphanumeric, hyphens, underscores, and dots
    return key.replace(/[^a-zA-Z0-9._-]/g, "_");
  }

  /**
   * Check if error is a quota exceeded error
   */
  private isQuotaExceededError(error: unknown): boolean {
    if (error instanceof DOMException) {
      return (
        error.code === 22 ||
        error.code === 1014 ||
        error.name === "QuotaExceededError" ||
        error.name === "NS_ERROR_DOM_QUOTA_REACHED"
      );
    }
    return false;
  }
}
