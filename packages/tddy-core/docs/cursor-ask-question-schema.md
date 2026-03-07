# Cursor AskQuestion Tool Call Schema

Captured from Cursor agent stream-json format. The Cursor CLI did not emit AskQuestion in live runs (it used plain text instead); this schema is inferred from the tool_call pattern used by other Cursor tools (shellToolCall, readToolCall, globToolCall, etc.).

## Event format

```json
{
  "type": "tool_call",
  "subtype": "started",
  "call_id": "t1",
  "tool_call": {
    "askUserQuestionToolCall": {
      "args": {
        "questions": [
          {
            "question": "Which tech stack?",
            "header": "Tech Stack",
            "options": [
              {"label": "React", "description": "React with hooks"},
              {"label": "Vanilla", "description": "Vanilla JS"}
            ],
            "multiSelect": false
          }
        ]
      }
    }
  },
  "session_id": "s1"
}
```

## Alternative tool name

Cursor may also use `askQuestionToolCall` (without "User"):

```json
{
  "tool_call": {
    "askQuestionToolCall": {
      "args": {
        "questions": [...]
      }
    }
  }
}
```

## Question structure

Same as Claude's AskUserQuestion `input`:

- `question` (string): The question text
- `header` (string): Section/category for display
- `options` (array): `{label, description}` for each choice
- `multiSelect` (boolean): Whether multiple options can be selected

## Implementation

`packages/tddy-core/src/stream/cursor.rs` extracts questions from both `askUserQuestionToolCall` and `askQuestionToolCall`, reusing `parse_ask_user_question()` from the Claude stream parser.
