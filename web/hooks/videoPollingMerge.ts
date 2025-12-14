import type { UserVideo } from "@/hooks/useVideoPolling";

function getVideoId(video: UserVideo): string {
  return video.video_id ?? video.id ?? "";
}

export function mergeProcessingStatuses(
  currentVideos: UserVideo[],
  updates: Array<{
    video_id: string;
    status?: UserVideo["status"];
    clips_count?: number;
    updated_at?: string;
  }>
): {
  merged: UserVideo[];
  completedVideoIds: string[];
  hadAnyChange: boolean;
} {
  const updatesById = new Map<string, (typeof updates)[number]>();
  updates.forEach((u) => updatesById.set(u.video_id, u));

  let hadAnyChange = false;
  const completedVideoIds: string[] = [];

  const merged = currentVideos.map((v) => {
    const id = getVideoId(v);
    if (!id) return v;

    const update = updatesById.get(id);
    if (!update) return v;

    const next: UserVideo = {
      ...v,
      status: update.status ?? v.status,
      clips_count: update.clips_count ?? v.clips_count,
      updated_at: update.updated_at ?? v.updated_at,
    };

    if (v.status !== next.status) {
      hadAnyChange = true;
      if (v.status === "processing" && next.status === "completed") {
        completedVideoIds.push(id);
      }
    }

    return next;
  });

  return { merged, completedVideoIds, hadAnyChange };
}
