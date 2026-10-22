/**
 * CardCraft In-Browser Simulator
 * Mirrors the Rust + Steel Scheme game loop directly in pure JS for the Pages showcase.
 */

class WebCardCraft {
  constructor() {
    this.reset();
    this.lastFeedback = "Type 's 1 2' to begin harvesting";
    this.intervalId = null;
  }

  reset() {
    this.moon = 1;
    this.dayDuration = 30.0;
    this.dayTimeLeft = 30.0;
    this.gold = 5;
    this.status = "RUNNING";
    this.nextId = 6;
    this.logs = [
      "Welcome to CardCraft! Stack Villager onto BerryBush (s 1 2).",
    ];

    this.stacks = [
      { id: 1, cards: ["Villager"], progress: null, total: null, recipe: null, status: "Idle" },
      { id: 2, cards: ["BerryBush"], progress: null, total: null, recipe: null, status: "Idle" },
      { id: 3, cards: ["Berry"], progress: null, total: null, recipe: null, status: "Idle" },
      { id: 4, cards: ["Wood"], progress: null, total: null, recipe: null, status: "Idle" },
      { id: 5, cards: ["Coin"], progress: null, total: null, recipe: null, status: "Idle" },
    ];

    this.recipes = [
      { name: "Harvest Berry", inputs: ["Villager", "BerryBush"], dur: 4.0, outputs: ["Berry"], consumed: [] },
      { name: "Chop Tree", inputs: ["Villager", "Tree"], dur: 5.0, outputs: ["Wood"], consumed: [] },
      { name: "Mine Rock", inputs: ["Villager", "Rock"], dur: 5.0, outputs: ["Stone"], consumed: [] },
      { name: "Cook Berry", inputs: ["Campfire", "Berry"], dur: 3.0, outputs: ["CookedBerry"], consumed: ["Berry"] },
      { name: "Build Campfire", inputs: ["Villager", "Wood", "Wood", "Stone"], dur: 6.0, outputs: ["Campfire"], consumed: ["Wood", "Wood", "Stone"] },
      { name: "Build Shed", inputs: ["Villager", "Wood", "Stone", "Stone"], dur: 6.0, outputs: ["Shed"], consumed: ["Wood", "Stone", "Stone"] },
    ];
  }

  addLog(msg) {
    this.logs.unshift(msg);
    if (this.logs.length > 5) this.logs.pop();
  }

  getFoodCurrent() {
    let sum = 0;
    for (const s of this.stacks) {
      for (const c of s.cards) {
        if (c === "Berry" || c === "Apple") sum += 1;
        if (c === "CookedBerry") sum += 2;
      }
    }
    return sum;
  }

  getFoodRequired() {
    let count = 0;
    for (const s of this.stacks) {
      for (const c of s.cards) {
        if (c === "Villager") count += 1;
      }
    }
    return count;
  }

  getCardCount() {
    return this.stacks.reduce((acc, s) => acc + s.cards.length, 0);
  }

  getCardLimit() {
    let sheds = 0;
    for (const s of this.stacks) {
      for (const c of s.cards) {
        if (c === "Shed") sheds += 1;
      }
    }
    return 15 + sheds * 5;
  }

  findRecipe(cards) {
    for (const r of this.recipes) {
      const copy = [...cards];
      let ok = true;
      for (const inp of r.inputs) {
        const idx = copy.indexOf(inp);
        if (idx === -1) { ok = false; break; }
        copy.splice(idx, 1);
      }
      if (ok) return r;
    }
    return null;
  }

  refreshStackRecipe(s) {
    const rec = this.findRecipe(s.cards);
    if (rec) {
      if (s.recipe === rec.name && s.progress !== null) {
        // preserve progress
      } else {
        s.progress = 0.0;
        s.total = rec.dur;
        s.recipe = rec.name;
        s.status = "Crafting";
      }
    } else {
      s.progress = null;
      s.total = null;
      s.recipe = null;
      s.status = "Idle";
    }
  }

  spawnCard(card) {
    const id = this.nextId++;
    const s = { id, cards: [card], progress: null, total: null, recipe: null, status: "Idle" };
    this.refreshStackRecipe(s);
    this.stacks.push(s);
  }

  tick(dt) {
    if (this.status !== "RUNNING") return;

    const pendingSpawns = [];
    for (const s of this.stacks) {
      if (s.progress !== null && s.recipe) {
        s.progress += dt;
        if (s.progress >= s.total) {
          const rec = this.findRecipe(s.cards);
          if (rec) {
            this.addLog(`Produced: ${rec.outputs.join(", ")}`);
            for (const out of rec.outputs) pendingSpawns.push(out);
            for (const cons of rec.consumed) {
              const idx = s.cards.indexOf(cons);
              if (idx !== -1) s.cards.splice(idx, 1);
            }
          }
          this.refreshStackRecipe(s);
        }
      }
    }

    this.stacks = this.stacks.filter(s => s.cards.length > 0);
    for (const card of pendingSpawns) {
      this.spawnCard(card);
    }

    this.dayTimeLeft -= dt;
    if (this.dayTimeLeft <= 0) {
      const req = this.getFoodRequired();
      const curr = this.getFoodCurrent();
      if (curr < req) {
        this.addLog("WARNING: Not enough food! A villager starved!");
      } else {
        this.addLog(`Moon ended! Consumed ${req} food.`);
        let toConsume = req;
        for (const s of this.stacks) {
          if (toConsume <= 0) break;
          const remaining = [];
          for (const c of s.cards) {
            if (toConsume > 0 && (c === "Berry" || c === "CookedBerry" || c === "Apple")) {
              const val = c === "CookedBerry" ? 2 : 1;
              toConsume -= val;
            } else {
              remaining.push(c);
            }
          }
          s.cards = remaining;
          this.refreshStackRecipe(s);
        }
        this.stacks = this.stacks.filter(s => s.cards.length > 0);
      }
      this.moon += 1;
      this.dayTimeLeft = this.dayDuration;
      this.addLog(`=== Moon ${this.moon} Begun! ===`);
    }
  }

  execute(cmdText) {
    const trimmed = cmdText.trim();
    if (!trimmed) return "Empty command";

    if (trimmed.startsWith("(")) {
      this.spawnCard("Apple");
      this.addLog("Lisp God Mode: Spawned 1x Apple!");
      return "OK (Scheme eval)";
    }

    const tokens = trimmed.split(/\s+/);
    const verb = tokens[0].toLowerCase();
    const args = tokens.slice(1);

    switch (verb) {
      case "s":
      case "stack": {
        if (args.length < 2) return "Usage: s <from> <to>";
        const fromId = parseInt(args[0], 10);
        const toId = parseInt(args[1], 10);
        const sFrom = this.stacks.find(s => s.id === fromId);
        const sTo = this.stacks.find(s => s.id === toId);
        if (!sFrom) return `Stack ${args[0]} not found.`;
        if (!sTo) return `Stack ${args[1]} not found.`;
        if (fromId === toId) return "Cannot stack onto itself.";

        sTo.cards.push(...sFrom.cards);
        this.stacks = this.stacks.filter(s => s.id !== fromId);
        this.refreshStackRecipe(sTo);
        this.addLog(`Stacked [${fromId}] onto [${toId}].`);
        return "OK";
      }

      case "u":
      case "unstack": {
        if (args.length < 1) return "Usage: u <slot>";
        const id = parseInt(args[0], 10);
        const s = this.stacks.find(s => s.id === id);
        if (!s) return `Stack ${args[0]} not found.`;
        if (s.cards.length <= 1) return "Stack has only 1 card, cannot unstack.";
        const top = s.cards.pop();
        this.refreshStackRecipe(s);
        this.spawnCard(top);
        this.addLog(`Popped ${top} from [${id}] to new slot.`);
        return "OK";
      }

      case "m":
      case "move": {
        if (args.length < 2) return "Usage: m <from> <to>";
        const fromId = parseInt(args[0], 10);
        const toId = parseInt(args[1], 10);
        const sFrom = this.stacks.find(s => s.id === fromId);
        const sTo = this.stacks.find(s => s.id === toId);
        if (!sFrom) return `Source stack ${fromId} not found.`;
        if (sTo) return `Slot ${toId} occupied. Use 's' to stack.`;
        sFrom.id = toId;
        this.addLog(`Moved [${fromId}] to [${toId}].`);
        return "OK";
      }

      case "b":
      case "buy": {
        if (args.length < 1) return "Usage: b <basic|food>";
        const pack = args[0].toLowerCase();
        if (pack === "basic") {
          if (this.gold < 3) return "Not enough gold! Basic Pack costs 3G.";
          this.gold -= 3;
          this.spawnCard("Pack:Basic");
          this.addLog("Purchased Basic Pack (3G).");
          return "OK";
        } else if (pack === "food") {
          if (this.gold < 5) return "Not enough gold! Food Pack costs 5G.";
          this.gold -= 5;
          this.spawnCard("Pack:Food");
          this.addLog("Purchased Food Pack (5G).");
          return "OK";
        }
        return "Unknown pack. Options: basic, food";
      }

      case "o":
      case "open": {
        if (args.length < 1) return "Usage: o <slot>";
        const id = parseInt(args[0], 10);
        const s = this.stacks.find(s => s.id === id);
        if (!s) return `Stack ${args[0]} not found.`;
        if (s.cards.includes("Pack:Basic")) {
          this.stacks = this.stacks.filter(stk => stk.id !== id);
          this.spawnCard("Wood");
          this.spawnCard("Stone");
          this.spawnCard("Berry");
          this.addLog("Opened Basic Pack! Got Wood, Stone, Berry.");
          return "OK";
        }
        if (s.cards.includes("Pack:Food")) {
          this.stacks = this.stacks.filter(stk => stk.id !== id);
          this.spawnCard("Berry");
          this.spawnCard("Berry");
          this.spawnCard("Apple");
          this.addLog("Opened Food Pack! Got 2x Berry, Apple.");
          return "OK";
        }
        return "No unopened pack in this stack.";
      }

      case "x":
      case "sell": {
        if (args.length < 1) return "Usage: x <slot>";
        const id = parseInt(args[0], 10);
        const s = this.stacks.find(s => s.id === id);
        if (!s) return `Stack ${args[0]} not found.`;
        const top = s.cards.pop();
        let val = 1;
        if (top === "Villager" || top === "Baby") {
          s.cards.push(top);
          return `Cannot sell ${top} (0G).`;
        }
        if (top === "CookedBerry") val = 2;
        if (top === "Campfire") val = 3;
        if (top === "Shed") val = 4;
        this.gold += val;
        this.addLog(`Sold ${top} for ${val}G.`);
        if (s.cards.length === 0) {
          this.stacks = this.stacks.filter(stk => stk.id !== id);
        } else {
          this.refreshStackRecipe(s);
        }
        return "OK";
      }

      case "w":
      case "wait": {
        const dt = args.length ? parseFloat(args[0]) : 1.0;
        this.tick(dt);
        this.addLog(`Waited ${dt}s.`);
        return "OK";
      }

      case "p":
      case "pause": {
        this.status = this.status === "RUNNING" ? "PAUSED" : "RUNNING";
        return this.status;
      }

      case "h":
      case "help":
      case "?":
        return "Commands: s <from> <to> | u <id> | m <from> <to> | b <pack> | o <id> | x <id> | w [s] | p";

      default:
        return `Unknown CAD command: ${verb}. Type 'h' for help.`;
    }
  }

  render() {
    const width = 80;
    const border = "─".repeat(width - 2);

    // Header
    const dayRatio = (this.dayDuration - this.dayTimeLeft) / this.dayDuration;
    const filled = Math.min(10, Math.round(dayRatio * 10));
    const dayBar = `[${"=".repeat(filled)}${".".repeat(10 - filled)}]`;
    const foodStr = `Food: ${this.getFoodCurrent()}/${this.getFoodRequired()}`;
    const cardStr = `Cards: ${this.getCardCount()}/${this.getCardLimit()}`;
    const statusStr = `[${this.status}]`;

    const metaLine = ` MOON ${this.moon}  | Day: ${dayBar} ${Math.round(this.dayTimeLeft)}s | ${foodStr} | Gold: ${this.gold}G | ${cardStr} | ${statusStr}`;

    let out = `┌ CardCraft (Console Stacklands) ${border.slice(32)}┐\n`;
    out += `│ ${metaLine.padEnd(width - 4)} │\n`;
    out += `└${border}┘\n`;

    // Stacks Board
    out += `┌ Active Stacks (Each Line = One Stack of Cards) ${border.slice(48)}┐\n`;
    if (this.stacks.length === 0) {
      out += `│ ${"No cards on board. Type 'b basic' to buy a pack.".padEnd(width - 4)} │\n`;
    } else {
      for (const s of this.stacks) {
        let line = `[${String(s.id).padStart(2, " ")}] ${s.cards.join(" ")}`;
        if (s.progress !== null && s.total) {
          const pRatio = Math.min(1, s.progress / s.total);
          const bFill = Math.round(pRatio * 8);
          const bar = `[${"=".repeat(bFill)}${".".repeat(8 - bFill)}]`;
          const leftSec = Math.max(0, (s.total - s.progress)).toFixed(1);
          line += `  ${bar} (${leftSec}s / ${s.recipe})`;
        } else if (s.status !== "Idle") {
          line += ` [${s.status}]`;
        }
        out += `│ ${line.padEnd(width - 4)} │\n`;
      }
    }
    // padding lines
    for (let i = this.stacks.length; i < 7; i++) {
      out += `│ ${" ".repeat(width - 4)} │\n`;
    }
    out += `└${border}┘\n`;

    // Shop
    out += `┌ Quick Store & Shortcuts ${border.slice(26)}┐\n`;
    out += `│ ${"SHOP: [b basic] Basic Pack (3G)  |  [b food] Food Pack (5G)   |  CAD: s, u, m, o, x, w, p".padEnd(width - 4)} │\n`;
    out += `└${border}┘\n`;

    // Logs
    out += `┌ Event Log ${border.slice(12)}┐\n`;
    for (let i = 0; i < 3; i++) {
      const log = this.logs[i] ? `• ${this.logs[i]}` : " ";
      out += `│ ${log.padEnd(width - 4)} │\n`;
    }
    out += `└${border}┘\n`;

    // Prompt
    const title = this.lastFeedback ? ` Command Prompt [${this.lastFeedback}] ` : " Command Prompt ";
    out += `┌${title}${border.slice(title.length)}┐\n`;
    out += `│ Command: ${" ".repeat(width - 15)} │\n`;
    out += `└${border}┘`;

    return out;
  }
}

// Global instance
const game = new WebCardCraft();
const screenEl = document.getElementById("terminal-screen");
const inputEl = document.getElementById("cli-input");
const btnSend = document.getElementById("btn-send");
const btnReset = document.getElementById("btn-reset");

function updateScreen() {
  if (screenEl) {
    screenEl.textContent = game.render();
  }
}

function handleInput() {
  if (!inputEl) return;
  const val = inputEl.value;
  if (!val.trim()) return;
  const fb = game.execute(val);
  game.lastFeedback = fb;
  inputEl.value = "";
  updateScreen();
}

function executeHint(cmd) {
  if (inputEl) {
    inputEl.value = cmd;
    handleInput();
  }
}

if (inputEl) {
  inputEl.addEventListener("keydown", (e) => {
    if (e.key === "Enter") handleInput();
  });
}

if (btnSend) {
  btnSend.addEventListener("click", handleInput);
}

if (btnReset) {
  btnReset.addEventListener("click", () => {
    game.reset();
    game.lastFeedback = "Game reset to initial state.";
    updateScreen();
  });
}

// Start simulation tick loop (100ms)
setInterval(() => {
  game.tick(0.1);
  updateScreen();
}, 100);

// Initial render
updateScreen();
