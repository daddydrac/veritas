FROM python:3.11-slim
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
RUN apt-get update && apt-get install -y --no-install-recommends curl poppler-utils tesseract-ocr && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY services/ingestion/requirements.txt /app/requirements.txt
RUN pip install --no-cache-dir -r /app/requirements.txt
COPY services/ingestion/veritas_ingest /app/veritas_ingest
COPY config /app/config
ENV PYTHONPATH=/app
CMD ["python", "-m", "veritas_ingest.cli", "--help"]
