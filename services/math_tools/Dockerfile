FROM python:3.11-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    VERITAS_MATH_TOOLS_PORT=8091

WORKDIR /workspace
COPY services/math_tools/requirements.txt /tmp/requirements.txt
RUN pip install --no-cache-dir -r /tmp/requirements.txt
COPY services/math_tools /workspace/services/math_tools
EXPOSE 8091
CMD ["sh", "-c", "uvicorn services.math_tools.app:app --host 0.0.0.0 --port ${VERITAS_MATH_TOOLS_PORT:-8091}"]
