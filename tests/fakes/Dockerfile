FROM python:3.11-slim
WORKDIR /app
RUN pip install --no-cache-dir fastapi uvicorn pydantic
COPY fake_vllm_server.py /app/fake_vllm_server.py
EXPOSE 8000
CMD ["uvicorn", "fake_vllm_server:app", "--host", "0.0.0.0", "--port", "8000"]
