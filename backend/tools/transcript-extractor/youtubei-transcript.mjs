#!/usr/bin/env node
import { Innertube } from "youtubei.js";

const videoUrl = process.argv[2];

if (!videoUrl) {
  console.error("Missing video URL argument");
  process.exit(2);
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

  const youtube = await Innertube.create();
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
    })
  );
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
