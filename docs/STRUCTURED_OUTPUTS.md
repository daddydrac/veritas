
# Using Structured Outputs with vLLM 0.20.1

This tutorial demonstrates how to produce structured outputs using vLLM version 0.20.1. Structured outputs allow you to constrain a language-model’s generation to a predefined format such as a JSON schema, a fixed choice list, a regular expression or a context-free grammar.

## Set up vLLM with structured outputs

Install vLLM plus dependencies like `openai` and `pydantic`:

```bash
pip install "vllm==0.20.1" openai pydantic
````

Start the vLLM server with structured-output support:

```bash
vllm serve meta-llama/Llama-3.1-8B-Instruct \
  --host 0.0.0.0 \
  --port 8000 \
  --structured-outputs-config.backend=auto
```

The `--structured-outputs-config.backend` option selects the backend used to enforce constraints. The default value `auto` chooses a backend automatically; you can override it based on performance or features.

