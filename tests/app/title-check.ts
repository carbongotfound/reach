// node tests/app/title-check.ts — fails loudly if task titles regress.
import assert from "node:assert/strict";
import { taskTitle } from "../../app/src/title.ts";

const cases: [string, string][] = [
  ["Open Notepad and write hello, then save it to the Desktop as hello", "Write hello in Notepad"],
  ["Open Excel (or Notepad if Excel is not installed), and enter these three invoices into a table: Acme 1200, Northwind 850, Contoso 430. Then save the file.", "Enter three invoices into a table"],
  ["Fill in the test form and stop before submitting", "Fill in the test form"],
  ["Please can you rename the file in the test window", "Rename the file in the test window"],
  ["In the window titled “Reach Test Pad”, type hello and press Ctrl+S", "Type hello"],
  ["Rename every photo in Downloads from this week by the date it was taken and move them to Pictures", "Rename every photo in Downloads…"],
  ["Work out 15% of 2480 in Calculator and write it in Notepad", "Work out 15% of 2480 in Calculator"],
  ["Find the oven temperature in the recipe open in Edge and write it in Notepad", "Find the oven temperature…"],
  ["Draw a blue rectangle in Paint", "Draw a blue rectangle in Paint"],
  ["In VS Code, write a Python function that adds up a list of prices and save it as prices.py", "Write a Python function that adds up…"],
  ["Find the euro sign in Character Map and tell me its Unicode code", "Find the euro sign in Character Map"],
];
for (const [task, want] of cases) assert.equal(taskTitle(task), want, task);
for (const [task] of cases) assert.ok(taskTitle(task).length <= 40);
console.log(`title-check: ${cases.length} ok`);
