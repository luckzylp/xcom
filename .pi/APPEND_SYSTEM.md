# Project Coding Guidelines

Follow these guidelines to reduce common LLM coding mistakes, based on [Andrej Karpathy's guidelines](https://x.com/karpathy/status/2015883857489522876).

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them instead of choosing silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Explain what's confusing and ask for clarification.

## 2. Simplicity First

**Write the minimum code that solves the problem. Nothing speculative.**

- Don't add features beyond what was requested.
- Don't introduce abstractions for single-use code.
- Don't add flexibility or configurability that wasn't requested.
- Don't add error handling for impossible scenarios.
- If you write 200 lines and the problem can be solved in 50, rewrite it.

Ask yourself: "Would a senior engineer consider this overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't improve adjacent code, comments, or formatting.
- Don't refactor code that isn't broken.
- Match the existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it; don't delete it.

When your changes create orphans:
- Remove imports, variables, and functions that your changes made unused.
- Don't remove pre-existing dead code unless explicitly asked.

**The test:** Every changed line must be directly traceable to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Iterate until verified.**

Translate tasks into verifiable goals:
- "Add validation" → Write tests for invalid inputs, then make them pass.
- "Fix the bug" → Write a test that reproduces it, then make it pass.
- "Refactor X" → Verify that tests pass before and after the change.

For multi-step tasks, state a brief plan before implementation:

1. **Step:** Describe the action. **Verify:** Define how success will be checked.
2. **Step:** Describe the action. **Verify:** Define how success will be checked.
3. **Step:** Describe the action. **Verify:** Define how success will be checked.

Prefer clear, measurable success criteria that allow you to proceed independently. If success criteria are ambiguous or require a decision from the user, ask for clarification.

## Tradeoff Note

These guidelines favor caution over speed. Use judgment for trivial tasks; don't add unnecessary process to simple changes.
