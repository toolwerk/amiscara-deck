#!/usr/bin/env python3
"""Spike: answer AskUserQuestion from a PermissionRequest hook.
The answer to give comes from out/question-answer.json:
{"sleep": 4, "answers": {"<question>": "<label>"} or "pick": [indexes per question]}"""
import json, os, sys, time
OUT = os.environ["DECK_Q_OUT"]
data = json.load(sys.stdin)
open(os.path.join(OUT, f"q-{int(time.time()*1000)}.json"), "w").write(json.dumps(data))
if data.get("hook_event_name") != "PermissionRequest" or data.get("tool_name") != "AskUserQuestion":
    sys.exit(0)
cfg = json.load(open(os.path.join(OUT, "question-answer.json")))
time.sleep(cfg.get("sleep", 0))
questions = data["tool_input"]["questions"]
answers = {}
for q, picks in zip(questions, cfg["pick"]):
    labels = [q["options"][i]["label"] for i in picks]
    answers[q["question"]] = ", ".join(labels) if q.get("multiSelect") else labels[0]
updated = dict(data["tool_input"], answers=answers)
print(json.dumps({"hookSpecificOutput": {"hookEventName": "PermissionRequest",
      "decision": {"behavior": "allow", "updatedInput": updated}}}))
open(os.path.join(OUT, "answered"), "w").write(str(time.time()))
