# REA на JS-бандле: рецепт, который укладывается в токены

**Задача.** Получить структуру минифицированного бандла через `rea analyze-javascript-application`,
не утопив контекст в 32,6 млн токенов.

**Измерено на `obsidian.asar` → `app.js` (3 172 382 байта, 1 строка):**

| Вариант | Токенов | Время |
|---|---:|---:|
| Без фильтра | **32 626 298** | 50 с |
| `nodes[0,1000].identity` + relations + coverage | 29 919 | 40 с |
| `nodes[0,2000].observations[0,1].label` (json) | 12 710 | 40 с |
| то же в `--format toon` | **6 760** | 40 с |
| kinds + relations в `toon` | **3 008** | 40 с |
| Ориентация целиком (coverage + limitations + обе гистограммы) | **3 221** | 40 с |

Сжатие **1090×** без потери того, ради чего анализ запускался.

---

## 1. Ориентация — 3 команды, ~10k токенов

### 1.1 Что вообще в графе

```bash
rea analyze-javascript-application "app.asar" \
  --source-path app.js \
  --format toon \
  --filter-output "normalized_result.graph.coverage,normalized_result.graph.limitations,normalized_result.graph.nodes[0,2000].kind,normalized_result.graph.edges[0,2000].relation"
```

`3 221` токенов, 40 с. Отдаёт: статус покрытия, честные limitations и две колонки —
типы узлов и типы связей. Из колонок сразу собираются гистограммы (см. §2).

### 1.2 Как эти узлы называются

```bash
rea analyze-javascript-application "app.asar" \
  --source-path app.js \
  --format toon \
  --filter-output "normalized_result.graph.nodes[0,2000].observations[0,1].label"
```

`6 760` токенов. Для `javascript-module` label — ключ модуля (`4397`), для `asar-entry` — путь
(`lib/mermaid.min.js`), для `storage` / `endpoint` — имя. Это и есть инвентарь.

### 1.3 Тяжёлый вариант — с namespace и require-именем

```bash
rea analyze-javascript-application "app.asar" \
  --source-path app.js \
  --format toon \
  --filter-output "normalized_result.graph.nodes[0,1000].identity"
```

`~30 000` токенов. Нужен, только если требуются `namespace` (`webpack-bootstrap@0:module`) и
`artifact_sha256` на каждый узел. Для навигации хватает 1.1 + 1.2.

---

## 2. Гистограммы из колонок

TOON рендерит массив как таблицу `nodes[423]{kind}:` + значения построчно. Значит работает
обычный `awk | sort | uniq -c`:

```bash
rea ... --format toon --filter-output "normalized_result.graph.nodes[0,2000].kind" \
| awk '/nodes\[/{f=1;next} /edges\[/{f=0} f && NF' \
| sort | uniq -c | sort -rn
```

На Obsidian `app.js`:

```
    180  javascript-module
     70  asar-entry
     69  unknown
     41  artifact
     30  javascript-asset
     14  storage
     12  endpoint
      5  worker
      1  javascript-chunk
      1  electron-preload
```

```bash
rea ... --filter-output "normalized_result.graph.edges[0,2000].relation" \
| awk '/edges\[/{f=1;next} /^ *[a-z_]+:/{f=0} f && NF' \
| sort | uniq -c | sort -rn
```

```
    320  contains
    295  imports
     72  maps_to
     45  persists_to
     18  calls
      6  loads
      1  exposes
```

Дальше — точечно: `grep -n electron-preload` по TOON-файлу находит строку, по её номеру берётся
соответствующий label из второй колонки. Это уже shell, а не «попроси модель пересказать 114 МБ».

---

## 3. Подводные камни (все проверены)

### 3.1 `nodes.kind` возвращает `{}`

Проекция по массиву **не работает без явного среза**. `Filter.apply` в `incur` рекурсирует только
в объекты; для массива с ключом на следующем сегменте он делает `merge({}, array, …)` и получает
`array["kind"] === undefined`.

```
--filter-output "…nodes.kind"        → {"nodes":{}}
--filter-output "…nodes[0,6].kind"   → {"nodes":[{"kind":"asar-entry"},…]}
```

Всегда `[start,end]`.

### 3.2 Одиночный индекс `[0]` ломается

Парсер делает `inner.split(',')` и `Number(undefined)` → `NaN` → `slice(0, NaN)` → пустой массив.
Писать `observations[0,1].label`, не `observations[0].label`.

### 3.3 Два пути на один массив перетирают друг друга

`nodes[..].kind,nodes[..].observations[0,1].label` — второй `merge` перезапишет `nodes` целиком.
Поэтому kind и label — двумя вызовами (§1.1 и §1.2). Дешевле, чем один вызов на 30k.

### 3.4 Прогресс идёт в stderr

`{"rea_progress":…}` не попадает в stdout. `--format json` можно сразу отдавать в `jq`, а
`--token-count` возвращает чистое число. Проверено: файл 114 774 391 байт начинается с
`{"evidence_id":…`.

### 3.5 `--token-limit` и `--token-offset` несовместимы со structured output

REA сам об этом пишет: «Token windows cannot preserve structured output. Remove
`--token-limit`/`--token-offset` and use `--filter-output` or command pagination». То есть
единственный способ уменьшить структурированный вывод — `--filter-output`, а не окно токенов.

### 3.6 `location` у `javascript-module` не указывает на тело модуля

Модуль `4397` реально начинается на `char 86344` (`byte 90913`), REA отдаёт
`start.column = 86482` — сдвиг **+138**. Диапазон `86482..86647` пересекает концы модулей
4397, 6632 и 1218. Из-за этого по графу нельзя прыгнуть в исходный текст модуля — только
по `label` искать в файле вручную.

Похоже на сдвиг координатного пространства (например, префикс обёртки анализатора). Стоит
проверить: это единственное, чего не хватает для полноценной навигации «граф → код».

---

## 4. Что это дало по Obsidian

- **180 webpack-модулей** с ключами и `factory_require_name`, `source_sha256` и
  `structural_fingerprint_sha256` (алгоритм `babel-ast-v2`) — то есть модули можно сравнивать
  между версиями по структуре, а не по байтам.
- **295 рёбер `imports`** с разрешёнными целями (`resolved_path: "app.js#module:2854"`).
- **1 `electron-preload`** и ребро `exposes` (`mechanism: property:path`) — граница contextBridge.
- **5 `worker`**, 12 `endpoint`, 14 `storage`, 18 `calls` (`db.get`/`db.put`/`db.delete`, `fetch`).
- Отдельно: `main.js` (Electron main) даёт узлы `ipc-channel` и `ipc-handler`, которых в `app.js`
  нет. Если нужен IPC-контур — это второй `--source-path`, а не переанализ всего asar.

Каждый факт помечен `authority` (`ast-static-analysis` / `static-relationship-inference`) и
`confidence`, а `limitations` честно говорят, что bootstrap не исполнялся. Это ровно тот уровень
дисциплины, которого не хватает `compat.rs` в Vibecodium.

---

## 5. Итог

Правильный порядок работы с JS-бандлом:

1. `--token-count` на пустом фильтре — узнать масштаб (у Obsidian 32,6 млн).
2. `toon` + `coverage/limitations` + две колонки — 3k токенов, понять состав.
3. `toon` + `labels` — 7k токенов, получить инвентарь.
4. Дальше только shell: `awk`/`sort`/`uniq`/`grep` по TOON-файлу.

Три команды, ~10k токенов, 2 минуты — вместо одного вызова на 32,6 млн, после которого
контекст уже некуда девать.
