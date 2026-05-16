#!/usr/bin/env python3
"""Optional faster-whisper ASR sidecar for voice-mcp-host.

This is the high-performance backend for Windows/NVIDIA and advanced users.
The default cross-platform backend remains whisper.cpp.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import sys
import time
from pathlib import Path
from typing import Any

DEFAULT_MODEL = "h2oai/faster-whisper-large-v3-turbo"


def default_compute_type(device: str) -> str:
    return "float16" if device == "cuda" else "int8"


def local_appdata() -> Path:
    if os.name == "nt":
        root = Path(os.environ.get("LOCALAPPDATA") or Path.home() / "AppData" / "Local")
        return root / "voice-mcp-host"
    return Path.home() / ".local" / "share" / "voice-mcp-host"


def ok(request_id: str | None, result: dict[str, Any]) -> dict[str, Any]:
    payload: dict[str, Any] = {"ok": True, "result": result}
    if request_id is not None:
        payload["id"] = request_id
    return payload


def err(request_id: str | None, code: str, message: str, details: dict[str, Any] | None = None) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "ok": False,
        "error": {"code": code, "message": message, "recoverable": True, "details": details or {}},
    }
    if request_id is not None:
        payload["id"] = request_id
    return payload


def emit(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")), flush=True)


def preflight(payload: dict[str, Any] | None = None) -> dict[str, Any]:
    payload = payload or {}
    ctranslate2_version = None
    faster_whisper_available = False

    try:
        import ctranslate2  # type: ignore

        ctranslate2_version = getattr(ctranslate2, "__version__", "unknown")
    except Exception:
        ctranslate2_version = None

    try:
        import faster_whisper  # type: ignore  # noqa: F401

        faster_whisper_available = True
    except Exception:
        faster_whisper_available = False

    cuda_ok = False
    cuda_compute_types: list[str] = []
    cpu_compute_types: list[str] = []
    if ctranslate2_version:
        try:
            import ctranslate2  # type: ignore

            cuda_compute_types = sorted(ctranslate2.get_supported_compute_types("cuda"))
            cuda_ok = bool(cuda_compute_types)
        except Exception:
            cuda_ok = False
        try:
            import ctranslate2  # type: ignore

            cpu_compute_types = sorted(ctranslate2.get_supported_compute_types("cpu"))
        except Exception:
            cpu_compute_types = []

    requested_device = str(payload.get("device") or "cuda")
    requested_compute_type = str(payload.get("compute_type") or default_compute_type(requested_device))

    return {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "ctranslate2": ctranslate2_version,
        "faster_whisper_available": faster_whisper_available,
        "cuda_ok": cuda_ok,
        "cuda_compute_types": cuda_compute_types,
        "cpu_compute_types": cpu_compute_types,
        "model_cache_dir": str(local_appdata() / "faster-whisper-models"),
        "requested_device": requested_device,
        "requested_compute_type": requested_compute_type,
    }


def validate_wav_path(path_text: str) -> Path:
    path = Path(path_text)
    if not path.is_absolute():
        raise ValueError("wav_path must be absolute")
    if path.suffix.lower() != ".wav":
        raise ValueError("wav_path must point to a .wav file")
    if not path.exists():
        raise FileNotFoundError(path_text)
    return path


def transcribe(payload: dict[str, Any]) -> dict[str, Any]:
    wav_path = validate_wav_path(str(payload.get("wav_path", "")))
    start = time.perf_counter()

    try:
        from faster_whisper import WhisperModel  # type: ignore
    except Exception as exc:
        raise RuntimeError(f"faster-whisper unavailable: {exc}") from exc

    model_name = str(payload.get("model_name") or DEFAULT_MODEL)
    device = str(payload.get("device") or "cuda")
    compute_type = str(payload.get("compute_type") or default_compute_type(device))
    model = WhisperModel(
        model_name,
        device=device,
        compute_type=compute_type,
        download_root=str(local_appdata() / "faster-whisper-models"),
    )
    segments, info = model.transcribe(
        str(wav_path),
        language=payload.get("language") or "en",
        beam_size=int(payload.get("beam_size", 5)),
        vad_filter=bool(payload.get("vad_filter", True)),
        condition_on_previous_text=bool(payload.get("condition_on_previous_text", False)),
        temperature=float(payload.get("temperature", 0.0)),
    )
    text = "".join(segment.text for segment in segments).strip()
    return {
        "text": text,
        "text_length": len(text),
        "language": getattr(info, "language", payload.get("language") or "en"),
        "audio_duration_seconds": getattr(info, "duration", None),
        "transcription_seconds": round(time.perf_counter() - start, 3),
        "model_name": model_name,
        "device": device,
        "compute_type": compute_type,
    }


def handle_request(request: dict[str, Any]) -> dict[str, Any]:
    request_id = request.get("id")
    request_type = request.get("type")
    payload = request.get("payload") or {}
    if not isinstance(payload, dict):
        return err(request_id, "invalid_request", "payload must be an object")
    try:
        if request_type == "preflight":
            return ok(request_id, preflight(payload))
        if request_type == "transcribe":
            return ok(request_id, transcribe(payload))
        return err(request_id, "invalid_request", f"unsupported request type: {request_type}")
    except FileNotFoundError as exc:
        return err(request_id, "wav_not_found", f"WAV file not found: {exc}")
    except Exception as exc:
        return err(request_id, "transcription_failed", str(exc), {"type": type(exc).__name__})


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--transcribe")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--model", default=DEFAULT_MODEL)
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--compute-type", default="")
    parser.add_argument("--language", default="en")
    args = parser.parse_args(argv)

    if args.preflight:
        emit(ok(None, preflight({"device": args.device, "compute_type": args.compute_type})))
        return 0

    if args.transcribe:
        response = handle_request(
            {
                "id": "cli_transcribe",
                "type": "transcribe",
                "payload": {
                    "wav_path": str(Path(args.transcribe).resolve()),
                    "model_name": args.model,
                    "device": args.device,
                    "compute_type": args.compute_type,
                    "language": args.language,
                },
            }
        )
        emit(response)
        return 0 if response.get("ok") else 1

    parser.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
