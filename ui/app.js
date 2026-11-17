// Rust Tweaker — UI logic
const { invoke } = window.__TAURI__.core;

let ALL = [];            // список твиков с backend'а
let CAT = "all";         // активная категория
let QUERY = "";          // поисковый запрос

const $ = (sel) => document.querySelector(sel);

/* ================= ЭКРАН ВХОДА ================= */

const BOOT_STEPS = [
  { text: "Проверка прав администратора", action: () => invoke("is_admin") },
  { text: "Сканирование системы Windows", action: () => new Promise((r) => setTimeout(() => r(true), 500)) },
  { text: "Загрузка списка твиков", action: () => invoke("list_tweaks") },
];

async function bootSequence() {
  const card = $("#login-card");
  const list = $("#boot-steps");
  requestAnimationFrame(() => card.classList.add("in"));

  let tweaks = null;
  let admin = true;

  for (let i = 0; i < BOOT_STEPS.length; i++) {
    const li = document.createElement("li");
    li.innerHTML = `<span class="dot"></span><span>${BOOT_STEPS[i].text}...</span>`;
    list.appendChild(li);
    await sleep(90);
    li.classList.add("show", "active");
    await sleep(320);
    try {
      const res = await BOOT_STEPS[i].action();
      if (i === 0) admin = res === true;
      if (i === 2) tweaks = res;
      li.classList.remove("active");
      li.classList.add("done");
      li.querySelector("span:last-child").textContent = `${BOOT_STEPS[i].text} — OK`;
    } catch (e) {
      li.classList.remove("active");
      li.classList.add("fail");
      li.querySelector("span:last-child").textContent = `${BOOT_STEPS[i].text} — ошибка`;
    }
  }

  if (!admin) {
    const warn = $("#boot-warn");
    warn.textContent = "⚠ Приложение запущено без прав администратора — часть твиков будет недоступна.";
    warn.classList.remove("hidden");
  }
  if (tweaks) ALL = tweaks;

  const btn = $("#btn-enter");
  btn.classList.remove("hidden");
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

$("#btn-enter").addEventListener("click", () => {
  const card = $("#login-card");
  card.classList.add("out");
  setTimeout(() => {
    $("#screen-login").classList.add("hidden");
    $("#screen-app").classList.remove("hidden");
    requestAnimationFrame(() => $("#shell").classList.add("in"));
    render();
  }, 650);
});

/* ================= РЕНДЕР ================= */

const CAT_NAMES = { privacy: "Приватность", ui: "Интерфейс", perf: "Производительность" };

function render() {
  const wrap = $("#cards");
  wrap.innerHTML = "";
  const items = ALL.filter((t) => {
    const okCat = CAT === "all" || t.category === CAT;
    const q = QUERY.toLowerCase();
    const okQ = !q || t.name.toLowerCase().includes(q) || t.desc.toLowerCase().includes(q);
    return okCat && okQ;
  });
  items.forEach((t, i) => wrap.appendChild(cardEl(t, i)));
  updateCounts();
}

function cardEl(t, idx) {
  const el = document.createElement("div");
  el.className = "card";
  el.style.animationDelay = `${Math.min(idx * 35, 500)}ms`;
  el.dataset.id = t.id;

  const stateClass = t.state === "on" ? "status-on" : t.state === "off" ? "status-off" : "status-unknown";
  const stateText = t.state === "on" ? "Применён" : t.state === "off" ? "Не применён" : "Неизвестно";
  const on = t.state === "on";

  el.innerHTML = `
    <div class="card-top">
      <div>
        <div class="card-title">${t.name}</div>
        <span class="card-badge badge-${t.category}">${CAT_NAMES[t.category]}</span>
      </div>
      <span class="status-dot ${stateClass}"></span>
    </div>
    <div class="card-desc">${t.desc}</div>
    <div class="card-foot">
      <span class="card-state ${t.state === "on" ? "on" : ""}">${stateText}</span>
      <div class="switch ${on ? "on" : ""}" role="switch"><div class="knob"></div></div>
    </div>
  `;

  // 3D-наклон карточки под курсором
  el.addEventListener("mousemove", (e) => {
    const r = el.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width - 0.5;
    const y = (e.clientY - r.top) / r.height - 0.5;
    el.style.setProperty("--ry", `${x * 6}deg`);
    el.style.setProperty("--rx", `${-y * 6}deg`);
  });
  el.addEventListener("mouseleave", () => {
    el.style.setProperty("--rx", "0deg");
    el.style.setProperty("--ry", "0deg");
  });

  // тумблер
  const sw = el.querySelector(".switch");
  sw.addEventListener("click", async () => {
    const enable = !sw.classList.contains("on");
    sw.classList.add("busy");
    try {
      await invoke("apply_tweak", { id: t.id, enable });
      t.state = enable ? "on" : "off";
      sw.classList.toggle("on", enable);
      const dot = el.querySelector(".status-dot");
      dot.className = `status-dot ${enable ? "status-on" : "status-off"}`;
      const st = el.querySelector(".card-state");
      st.textContent = enable ? "Применён" : "Не применён";
      st.classList.toggle("on", enable);
      toast(enable ? `✓ ${t.name} — применён` : `↩ ${t.name} — сброшен`, "ok");
      if (["context_menu", "widgets", "animations", "transparency"].includes(t.id)) {
        toast("ℹ Рекомендуется перезапустить Explorer, чтобы изменения применились");
      }
      updateCounts();
    } catch (e) {
      toast(`✗ ${t.name}: ${prettyErr(e)}`, "err");
    } finally {
      sw.classList.remove("busy");
    }
  });

  return el;
}

function updateCounts() {
  $("#c-all").textContent = ALL.length;
  $("#c-privacy").textContent = ALL.filter((t) => t.category === "privacy").length;
  $("#c-ui").textContent = ALL.filter((t) => t.category === "ui").length;
  $("#c-perf").textContent = ALL.filter((t) => t.category === "perf").length;
  $("#stat-total").textContent = ALL.length;
  $("#stat-applied").textContent = ALL.filter((t) => t.state === "on").length;
}

function prettyErr(e) {
  const s = typeof e === "string" ? e : e?.message || JSON.stringify(e);
  return s.length > 120 ? s.slice(0, 120) + "…" : s;
}

/* ================= КАТЕГОРИИ / ПОИСК ================= */

$("#nav").addEventListener("click", (e) => {
  const btn = e.target.closest(".nav-btn");
  if (!btn) return;
  document.querySelectorAll(".nav-btn").forEach((b) => b.classList.remove("active"));
  btn.classList.add("active");
  CAT = btn.dataset.cat;
  render();
});

$("#search").addEventListener("input", (e) => {
  QUERY = e.target.value.trim();
  render();
});

/* ================= МАССОВЫЕ ОПЕРАЦИИ ================= */

async function applyAll(enable) {
  const overlay = $("#progress-overlay");
  const fill = $("#bar-fill");
  const sub = $("#progress-sub");
  const title = $("#progress-title");
  title.textContent = enable ? "Применение твиков..." : "Сброс твиков...";
  overlay.classList.remove("hidden");
  fill.style.width = "0%";

  // Индикация прогресса (backend применяет всё разом)
  let p = 0;
  const timer = setInterval(() => {
    p = Math.min(p + 4, 92);
    fill.style.width = `${p}%`;
  }, 120);

  let results = [];
  try {
    results = await invoke("apply_all", { enable });
  } catch (e) {
    clearInterval(timer);
    overlay.classList.add("hidden");
    toast(`✗ Ошибка массовой операции: ${prettyErr(e)}`, "err");
    return;
  }
  clearInterval(timer);
  fill.style.width = "100%";
  await sleep(250);
  overlay.classList.add("hidden");

  ALL = await invoke("list_tweaks");
  render();

  const ok = results.filter((r) => r.ok).length;
  const bad = results.filter((r) => !r.ok);
  if (bad.length === 0) {
    toast(`✓ Готово: ${ok} твиков ${enable ? "применено" : "сброшено"}`, "ok");
  } else {
    toast(`⚠ Готово: ${ok} успешно, ${bad.length} с ошибкой`, "err");
    bad.slice(0, 3).forEach((r) => toast(`✗ ${r.id}: ${r.message}`, "err"));
  }
}

$("#btn-apply-all").addEventListener("click", () => applyAll(true));
$("#btn-revert-all").addEventListener("click", () => applyAll(false));

$("#btn-explorer").addEventListener("click", async () => {
  try {
    const msg = await invoke("restart_explorer");
    toast(`✓ ${msg}`, "ok");
  } catch (e) {
    toast(`✗ ${prettyErr(e)}`, "err");
  }
});

$("#btn-restore").addEventListener("click", async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  toast("⏳ Создание точки восстановления (может занять минуту)...");
  try {
    const msg = await invoke("create_restore_point");
    toast(`✓ ${msg}`, "ok");
  } catch (err) {
    toast(`✗ Точка восстановления: ${prettyErr(err)}`, "err");
  } finally {
    btn.disabled = false;
  }
});

/* ================= ТОСТЫ ================= */

function toast(text, type = "") {
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  el.textContent = text;
  $("#toast-wrap").appendChild(el);
  setTimeout(() => {
    el.classList.add("bye");
    setTimeout(() => el.remove(), 350);
  }, 4200);
}

/* ================= СТАРТ ================= */
bootSequence();
