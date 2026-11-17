"""Generate the game's sound effects and music with the ElevenLabs API.

Usage (the key is read from the environment, never stored in the repo):

    ELEVENLABS_API_KEY=... python tools/generate_audio.py            # only missing files
    ELEVENLABS_API_KEY=... python tools/generate_audio.py hit miss   # regenerate these
    ELEVENLABS_API_KEY=... python tools/generate_audio.py --all      # regenerate everything
    add --full-music to compose 90-120 s tracks with the Music API (paid plans only)

Files land in assets/sounds/ as MP3 and are embedded into the game binary at build time.
"""

import json
import os
import sys
import urllib.error
import urllib.request

API = "https://api.elevenlabs.io/v1"
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "sounds")
FORMAT = "mp3_44100_128"

# Shared look of the game: dark, clean, minimalist, precise. Keep effects dry and short so they
# read instantly during play.
SFX = {
    "hit": (
        "Crisp satisfying target hit sound for a minimalist FPS aim trainer: a very short bright "
        "digital 'tink' pop with a tiny glassy click. Clean, dry, no reverb tail, no music.",
        0.5,
    ),
    "miss": (
        "Soft muted miss sound for an aim trainer: a very short low dull digital thud. Subtle and "
        "unobtrusive, clean, dry, no reverb, no music.",
        0.5,
    ),
    "countdown_tick": (
        "A single clean digital countdown tick: one short soft sine-like blip. Minimal UI sound, "
        "dry, no reverb, no music.",
        0.5,
    ),
    "countdown_go": (
        "Short bright start signal for a game round: a clean rising two-note digital chime. "
        "Energetic but minimal UI sound, dry, no music.",
        0.8,
    ),
    "spawn_cue": (
        "Short high-pitched alert blip telling the player a target appeared: two quick bright "
        "digital pings. Clean UI notification, dry, no reverb, no music.",
        0.5,
    ),
    "sens_change": (
        "Subtle short futuristic UI swoosh with a soft digital shimmer, a setting just changed. "
        "Minimal, clean, quiet, no music.",
        0.8,
    ),
    "complete": (
        "Short satisfying success chime for finishing a training session in a sleek minimalist "
        "sci-fi game: three ascending clean synth notes with a soft sparkle. No vocals.",
        1.5,
    ),
}

# Music loops come from the sound-effects model in loop mode (30 s maximum): the Music API
# needs a paid ElevenLabs plan. With a paid plan, `--full-music` composes longer tracks instead.
MUSIC = {
    "music_menu": (
        "Calm minimalist electronic music loop for a sleek modern aim trainer game menu. Deep warm "
        "synth pads, soft pulsing sub bass, subtle glitchy hi-hats, focused and confident mood, "
        "100 BPM. Instrumental, no vocals, seamless loop.",
        90_000,
    ),
    "music_game": (
        "Focused minimal techno music loop for an aim training session. Tight steady kick at "
        "120 BPM, clean pulsing synth arpeggio, low-key and non-distracting, no vocals, no drops, "
        "seamless loop.",
        120_000,
    ),
}
LOOP_SECS = 30.0


def post(path: str, body: dict, key: str) -> bytes:
    req = urllib.request.Request(
        f"{API}{path}?output_format={FORMAT}",
        data=json.dumps(body).encode(),
        headers={"xi-api-key": key, "Content-Type": "application/json", "Accept": "audio/mpeg"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=600) as r:
        return r.read()


FULL_MUSIC = False


def generate(name: str, key: str) -> None:
    if name in SFX:
        text, secs = SFX[name]
        audio = post(
            "/sound-generation",
            {"text": text, "duration_seconds": secs, "prompt_influence": 0.6},
            key,
        )
    elif FULL_MUSIC:
        prompt, ms = MUSIC[name]
        audio = post(
            "/music",
            {"prompt": prompt, "music_length_ms": ms, "model_id": "music_v1", "force_instrumental": True},
            key,
        )
    else:
        prompt, _ = MUSIC[name]
        audio = post(
            "/sound-generation",
            {
                "text": prompt,
                "duration_seconds": LOOP_SECS,
                "prompt_influence": 0.5,
                "loop": True,
                "model_id": "eleven_text_to_sound_v2",
            },
            key,
        )
    path = os.path.join(OUT_DIR, f"{name}.mp3")
    with open(path, "wb") as f:
        f.write(audio)
    print(f"{name}: {len(audio) / 1024:.0f} KB")


def main() -> int:
    key = os.environ.get("ELEVENLABS_API_KEY")
    if not key:
        print("Set ELEVENLABS_API_KEY first.", file=sys.stderr)
        return 2
    os.makedirs(OUT_DIR, exist_ok=True)
    every = list(SFX) + list(MUSIC)
    args = sys.argv[1:]
    global FULL_MUSIC
    FULL_MUSIC = "--full-music" in args
    args = [a for a in args if a != "--full-music"]
    if "--all" in args:
        names = every
    elif args:
        unknown = [a for a in args if a not in every]
        if unknown:
            print(f"Unknown: {unknown}. Known: {every}", file=sys.stderr)
            return 2
        names = args
    else:
        names = [n for n in every if not os.path.exists(os.path.join(OUT_DIR, f"{n}.mp3"))]
    failed = 0
    for name in names:
        try:
            generate(name, key)
        except urllib.error.HTTPError as e:
            failed += 1
            print(f"{name}: HTTP {e.code} {e.read().decode(errors='replace')[:400]}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
