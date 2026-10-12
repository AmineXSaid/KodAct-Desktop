# Set up an endpoint

Forge has no model built in. You tell it where your models are: a local server
such as Ollama, a hosted API, or a gateway. Each entry, a **profile**, is one
endpoint together with the one model it runs. It takes about two minutes.

## 1. Open endpoints.json

In Forge, open the model menu under the composer (**Ctrl+/**) and choose
**Open endpoints.json**. If the file doesn't exist yet, Forge creates it with
one local profile to start from, then opens it in your editor.

| System | Where the file lives |
|:--|:--|
| Windows | `%APPDATA%\Forge\endpoints.json` |
| macOS | `~/Library/Application Support/Forge/endpoints.json` |
| Linux | `~/.config/forge/endpoints.json` |

## 2. Add a profile

Copy the block that matches your endpoint into `profiles`, and change the
model to one your endpoint serves.

### A model on this PC (Ollama, LM Studio, llama.cpp, vLLM)

No key needed. Ollama listens on port 11434, LM Studio on 1234.

```json
{
  "name": "ollama",
  "baseUrl": "http://127.0.0.1:11434",
  "wire": "openai",
  "model": "qwen3-coder:30b",
  "displayName": "Qwen3 Coder"
}
```

### A hosted API that speaks OpenAI's format (DeepSeek, OpenRouter, Together…)

```json
{
  "name": "deepseek",
  "baseUrl": "https://api.deepseek.com",
  "wire": "openai",
  "model": "deepseek-chat",
  "displayName": "DeepSeek V3.2",
  "auth": { "type": "bearer", "token": "${env:DEEPSEEK_API_KEY}" },
  "capabilities": { "contextWindow": 131072 }
}
```

For OpenRouter, use `"baseUrl": "https://openrouter.ai/api/v1"` and a model id
from its catalogue, such as `"qwen/qwen3-coder"`.

### An endpoint that speaks Anthropic's format (a gateway or proxy)

```json
{
  "name": "gateway",
  "baseUrl": "http://127.0.0.1:4000",
  "wire": "anthropic",
  "model": "qwen3-coder",
  "auth": { "type": "bearer", "token": "${env:GATEWAY_KEY}" }
}
```

A complete file with two profiles looks like this:

```json
{
  "selected": "ollama",
  "syncIntervalMinutes": 5,
  "profiles": [
    { "name": "ollama", "baseUrl": "http://127.0.0.1:11434", "wire": "openai", "model": "qwen3-coder:30b" },
    { "name": "deepseek", "baseUrl": "https://api.deepseek.com", "wire": "openai", "model": "deepseek-chat",
      "auth": { "type": "bearer", "token": "${env:DEEPSEEK_API_KEY}" } }
  ]
}
```

## 3. Give it its key, outside the file

A key is never written into `endpoints.json`. The profile names an environment
variable, `${env:NAME}`, and Forge reads the key from there. Forge refuses a
key typed into the file.

On Windows, in a terminal:

```powershell
setx DEEPSEEK_API_KEY "sk-your-key"
```

Then quit and reopen Forge, so it sees the new variable. On macOS or Linux, add
`export DEEPSEEK_API_KEY="sk-your-key"` to your shell profile, and start Forge
from a new terminal.

## 4. Check it

Save the file, open the model menu and press **refresh** (the circling
arrows). Forge reads `endpoints.json` again and sends each model a one-token
request:

- [x] A time chip, such as `184ms`, means it answered: green when quick, grey when fair, amber when slow.
- [ ] A model that didn't answer is left out, so the list only offers what works. The one you picked stays, greyed, with the reason under it, such as `did not answer: HTTP 401`.

To see why a profile is missing, set `"selected"` to its name in
`endpoints.json` and press refresh again: its row then says why. Pick a model
that answers, and it is the one your chats use. Forge remembers it.

## Every field

| Field | Needed | What it does |
|:--|:-:|:--|
| `name` | yes | The profile's name; one per profile. Shown under the model. |
| `baseUrl` | yes | Where the endpoint is, such as `http://127.0.0.1:11434`. |
| `wire` | | `openai` or `anthropic`: the format it speaks. `anthropic` when left out. |
| `model` | yes | The model id the endpoint knows it by. |
| `displayName` | | What the model menu calls it. The model id when left out. |
| `chatPath` | | A chat route other than the usual one. |
| `headers` | | Extra headers to send, such as `{ "HTTP-Referer": "https://forge.local" }`. |
| `auth` | | `{ "type": "bearer", "token": "${env:NAME}" }` when it needs a key. |
| `capabilities.contextWindow` | | The model's window in tokens, for the context meter. |
| `capabilities.effort` | | `true` if the model takes a reasoning effort. |
| `capabilities.effortLevels` | | Which: `low`, `medium`, `high`, `xhigh`, `max`. |
| `capabilities.fast` | | `true` if it has a fast mode. |
| `selected` | | The profile picked when Forge starts (top level). |
| `syncIntervalMinutes` | | How often every model is checked; `0` turns it off (top level). |

The context meter under the composer shows only when Forge knows the window:
from `contextWindow`, or from the endpoint's own model list when it states one.

## How Forge builds the URLs

| Wire | Chat | Model list |
|:--|:--|:--|
| `openai` | `baseUrl` + `/v1/chat/completions`, or `/chat/completions` when `baseUrl` already ends in `/v1` | `/v1/models` |
| `anthropic` | `baseUrl` + `/v1/messages`, without doubling a `/v1` already in `baseUrl` | `/v1/models` |

## When a model doesn't answer

| What Forge says | What to do |
|:--|:--|
| `could not be checked: the key ${env:NAME} is not set` | Set the variable (step 3), then reopen Forge. |
| `a key must be given as ${env:NAME}, not written into endpoints.json` | Move the key into an environment variable and put `${env:NAME}` in the file. |
| `did not answer: HTTP 401` or a message about the key | The key is wrong or expired: make a new one at your provider. |
| `could not be checked: could not connect` | Nothing is listening at `baseUrl`. Is the server running, and is the port right? |
| `could not be checked: the host could not be found` | The host name is misspelled, or you're offline. |
| `did not answer: listed, but accepted the request and never answered` | The server took the request but didn't reply in time: it may still be loading the model. Try refresh again. |
| `did not answer:` and a model error | The model id isn't one the endpoint serves. Check its list at `baseUrl/v1/models`. |

Forge waits 4 seconds to connect, 10 seconds for a hosted model to answer,
and 20 seconds for one on this PC, which may need to load first.
