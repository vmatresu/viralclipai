# Prompts & AI Behavior

This document explains how prompting works in Viral Clip AI and how to customize it.

## Prompt Sources

Prompt selection order for each job:

1. **User custom prompt** (per job, from the UI).
2. **Global admin base prompt** stored in Firestore.
3. **Local `prompt.txt`** in the project root (fallback for initial setups).

The effective `base_prompt` is passed into Google Gemini and controls how
highlights are found and described, but the JSON structure of the response
remains consistent.

## User Custom Prompts (UI)

On the main processing page, users can:

- Enter a free-form prompt in the "Custom prompt (optional)" textarea.
- Choose from example presets (emotional moments, best viral clips, etc.).

This prompt is sent over WebSocket as `prompt` and, if non-empty:

- Overrides the base prompt for that job.
- Is stored in `highlights.json` as `custom_prompt`.
- Is stored in Firestore on `users/{uid}/videos/{run_id}.custom_prompt`.
- Is displayed on the results page and in the history view.

## Global Admin Prompt

Super-admin users can manage a global base prompt that applies when a user does
not specify a custom prompt.

- Stored at `admin/config.base_prompt` in Firestore.
- Editable via the `/admin/prompt` page in the frontend.
  - Requires `role = "superadmin"` on the user document.
- Used as the default `base_prompt` for Gemini when no per-job custom prompt is
  provided.

## `prompt.txt` Fallback

The `prompt.txt` file in the project root is maintained as a local fallback for
cases where no global prompt is configured in Firestore.

- If no `custom_prompt` and no `admin/config.base_prompt` is defined, the
  contents of `prompt.txt` are used as `base_prompt`.
- This allows bootstrapping a new environment before Firestore admin settings
  are configured.

## Prompt Design Guidelines

When designing prompts (either globally or per job), keep these best practices
in mind:

- **Be explicit about the goal**
  - e.g. "Find the most emotionally intense, vulnerable moments that would
    resonate on TikTok and Instagram Reels."
- **Constrain clip length and structure**
  - e.g. "Prefer clips between 20 and 45 seconds with a strong hook in the
    first 3 seconds."
- **Describe platform context**
  - e.g. "Optimize for vertical, short-form content where viewers may swipe
    quickly if bored."
- **Call out forbidden content or low value segments**
  - e.g. "Avoid intro/outro fluff, sponsor reads, and long pauses."
- **Ask for metadata that the app uses**
  - e.g. "Provide a concise, compelling title and a short description for each
    highlight."

### Example Prompts

- **Emotional moments**

  > Find the most emotional and vulnerable moments in this video that would
  > resonate strongly on TikTok.

- **Best viral clips for social media**

  > Find the best high-retention viral clip candidates for short-form social
  > media (TikTok, Shorts, Reels).

- **Subject-focused discussion**

  > Find segments with intense discussion about the main subject, where there
  > is strong opinion or debate.

- **Sound-focused clips**

  > Find moments with interesting sounds or reactions that would work well in
  > sound-on social media clips.

Tailor the global prompt to your product's brand voice, then encourage users to
use custom prompts for specific campaigns or content types.
