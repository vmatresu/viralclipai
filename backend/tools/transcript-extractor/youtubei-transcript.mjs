#!/usr/bin/env node
/**
 * YouTube Transcript Extractor using youtubei.js
 * 
 * Features:
 * - Extracts transcripts with timestamps from YouTube videos
 * - IPv6 rotation support to avoid rate limiting (when available)
 * - Outputs JSON with transcript, segment count, and source
 */

import os from "node:os";
import { Agent, fetch as undiciFetch } from "undici";
import { Innertube } from "youtubei.js";

const videoUrl = process.argv[2];

if (!videoUrl) {
  console.error("Missing video URL argument");
  process.exit(2);
}

/**
 * Get a random global IPv6 address from available network interfaces.
 * 
 * Used for IP rotation to avoid YouTube rate limiting.
 * Filters out:
 * - IPv4 addresses
 * - Link-local addresses (fe80::)
 * - Loopback addresses (::1)
 * - Internal/unique local addresses (fc00::/7, fd00::/8)
 * 
 * @returns {string|null} A random global IPv6 address or null if none available
 */
function getRandomIPv6Address() {
  const interfaces = os.networkInterfaces();
  const globalAddresses = [];

  for (const [name, addrs] of Object.entries(interfaces)) {
    if (!addrs) continue;

    for (const addr of addrs) {
      // Only consider IPv6
      if (addr.family !== "IPv6") continue;

      // Skip internal/loopback
      if (addr.internal) continue;

      // Skip link-local (fe80::/10)
      if (addr.address.startsWith("fe80:")) continue;

      // Skip loopback
      if (addr.address === "::1") continue;

      // Skip unique local (fc00::/7 and fd00::/8)
      if (addr.address.startsWith("fc") || addr.address.startsWith("fd")) {
        continue;
      }

      globalAddresses.push(addr.address);
    }
  }

  if (globalAddresses.length === 0) {
    return null;
  }

  // Select random address
  const randomIndex = Math.floor(Math.random() * globalAddresses.length);
  const selected = globalAddresses[randomIndex];

  console.error(
    `[IPv6] Selected ${selected} (from ${globalAddresses.length} available)`
  );

  return selected;
}

/**
 * Create a custom fetch function that binds to a specific local address.
 * Uses undici's Agent with localAddress option.
 * 
 * @param {string} localAddress - The local IP address to bind to
 * @returns {Function} A fetch-compatible function
 */
function createBoundFetch(localAddress) {
  const agent = new Agent({
    connect: {
      localAddress,
    },
  });

  return (url, options = {}) => {
    return undiciFetch(url, {
      ...options,
      dispatcher: agent,
    });
  };
}

function extractVideoId(url) {
  try {
    const parsed = new URL(url.trim());
    const host = parsed.hostname.toLowerCase();

    if (host === "youtu.be") {
      return parsed.pathname.replace("/", "");
    }

    const vParam = parsed.searchParams.get("v");
    if (vParam) {
      return vParam;
    }

    const pathParts = parsed.pathname.split("/").filter(Boolean);
    const shortsIndex = pathParts.indexOf("shorts");
    if (shortsIndex !== -1 && pathParts[shortsIndex + 1]) {
      return pathParts[shortsIndex + 1];
    }

    const embedIndex = pathParts.indexOf("embed");
    if (embedIndex !== -1 && pathParts[embedIndex + 1]) {
      return pathParts[embedIndex + 1];
    }
  } catch {
    return null;
  }

  return null;
}

function formatTimestamp(ms) {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [
    hours.toString().padStart(2, "0"),
    minutes.toString().padStart(2, "0"),
    seconds.toString().padStart(2, "0"),
  ].join(":");
}

function buildTranscript(initialSegments) {
  const segments = initialSegments
    .map((seg) => ({
      startMs: Number(seg.start_ms ?? 0),
      endMs: Number(seg.end_ms ?? 0),
      text: seg.snippet?.text ?? "",
    }))
    .filter((seg) => seg.text && seg.text.length > 0 && seg.endMs >= seg.startMs)
    .sort((a, b) => a.startMs - b.startMs);

  let lastText = "";
  const lines = [];

  for (const seg of segments) {
    const text = seg.text.replace(/\s+/g, " ").trim();
    if (!text || text === lastText) {
      continue;
    }
    const ts = formatTimestamp(seg.startMs);
    lines.push(`[${ts}] ${text}`);
    lastText = text;
  }

  return {
    transcript: lines.join("\n"),
    segmentCount: lines.length,
  };
}

async function main() {
  const videoId = extractVideoId(videoUrl);
  if (!videoId) {
    throw new Error("Could not extract video ID from URL");
  }

  // Build Innertube options
  const innertubeOptions = {};

  // IPv6 rotation: bind to random global IPv6 address if available
  const ipv6Address = getRandomIPv6Address();
  if (ipv6Address) {
    innertubeOptions.fetch = createBoundFetch(ipv6Address);
  }

  const youtube = await Innertube.create(innertubeOptions);
  const info = await youtube.getInfo(videoId);
  const transcriptData = await info.getTranscript();

  const initialSegments =
    transcriptData?.transcript?.content?.body?.initial_segments;

  if (!Array.isArray(initialSegments) || initialSegments.length === 0) {
    throw new Error("Transcript segments not available");
  }

  const output = buildTranscript(initialSegments);

  if (!output.transcript || output.segmentCount === 0) {
    throw new Error("Transcript is empty after parsing");
  }

  process.stdout.write(
    JSON.stringify({
      transcript: output.transcript,
      segment_count: output.segmentCount,
      source: "youtubei.js",
      ipv6_address: ipv6Address || null,
    })
  );
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});

