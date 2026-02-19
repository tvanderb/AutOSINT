# scribe: Python + Whisper transcription service.
# Skeleton — built out in M5.

FROM python:3.13-slim

RUN apt-get update && apt-get install -y ffmpeg && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt 2>/dev/null || true

COPY . .

EXPOSE 8090

CMD ["python", "-m", "scribe"]
