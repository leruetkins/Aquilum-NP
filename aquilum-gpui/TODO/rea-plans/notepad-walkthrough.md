# Разбор notepad.exe через REA — запись прогона

Демонстрация полного цикла на безопасной цели: `samples/notepad.exe` (копия системного
блокнота, вынесенная из `System32`, чтобы пройти admission).

- **Дата:** 18.10.2026
- **Цель:** `D:\Naben\Github\REA\samples\notepad.exe`
- **sha256:** `baefafd4c558e8ec66a5772b8ac8dc18d51789ccaf97be5fc66cd63a71caf75c`
- **Провайдер:** Ghidra 12.1.4 (цепочка `rea-workflow 1` → `ghidra 12.1.4`)
- **Окружение:** Windows x64, локальный NTFS, `GHIDRA_INSTALL_DIR` + `JAVA_HOME` из User env

## Шаг 1. Обзор бинаря

```bash
rea inspect "D:/Naben/Github/REA/samples/notepad.exe" \
  --provider ghidra --format json
```

Результат (`evidence_id` `ev_cbd51a0b…fde641c3`, 134 с холодного импорта):

| Поле | Значение | Что означает |
|---|---|---|
| `subject.format` / `architecture` | `pe` / `x86_64` | 64-битный PE, определён по заголовку |
| `subject.digest.sha256` | `baefafd4…` | идентичность байтов, к которым относится вывод |
| `normalized_result.image_base` | `0x140000000` | база образа; адреса ниже — VA, не RVA |
| `normalized_result.segment_count` | `9` | Headers, .text, .rdata, .data, .pdata, .didat, .rsrc, .reloc, tdb |
| `.text` | `0x140001000`–`0x140025800` | 149 504 байта кода |
| `normalized_result.procedure_count` | `796` | распознанных процедур |
| `normalized_result.string_count` | `557` | размеченных строк |
| `authority` | `shipped-artifact` | вывод относится к поставленным байтам, не к исходникам |
| `confidence` | `derived` | это вычисление REA, а не прямое наблюдение провайдера |

В `limitations` честно перечислено, чего вывод не утверждает: headless-сессия Ghidra
содержит ровно одну импортированную программу, отмена запроса уничтожает эфемерную базу,
кодировка сообщается отдельно от способа терминирования строк.

## Шаг 2. Код одной функции

Точка входа считается из PE-заголовка без всяких движков:

```bash
node -e "const b=require('fs').readFileSync('samples/notepad.exe');const pe=b.readUInt32LE(0x3c);const o=pe+24;const m=b.readUInt16LE(o);const ep=b.readUInt32LE(o+16);const ib=m===0x20b?Number(b.readBigUInt64LE(o+24)):b.readUInt32LE(o+28);console.log('0x'+(ib+ep).toString(16))"
# → 0x140023e90
```

```bash
rea decompile "D:/Naben/Github/REA/samples/notepad.exe" 0x140023e90 \
  --provider ghidra --format json
```

```c
void entry(void)
{
  __security_init_cookie();
  FUN_140023d10();
  return;
}
```

## Шаг 3. От entry к реальной main

```bash
rea function "D:/Naben/Github/REA/samples/notepad.exe" 0x140023d10 \
  --provider ghidra --format json
```

130 с, ответ 270 КБ — в чат такое не читают, поэтому проекция:

| Поле | Значение |
|---|---|
| `procedure.signature` | `ulonglong __fastcall FUN_140023d10(void)` |
| тело | 345 байт, два несмежных диапазона |
| `callees` | 19: `__scrt_initialize_crt`, `_initterm_e`, `_initterm`, `__scrt_get_show_window_mode`, `_o__get_wide_winmain_command_line` и др. |
| `basic_blocks` | 21 |
| `unresolved_calls` | 1 — `0x140023e81`, «Instruction decoder identifies a call but ReferenceManager supplies no call destination» |

Это `__scrt_common_main_seh` — штатный запуск CRT. В конце он вызывает
`FUN_14000ad5c(hInstance, 0, cmdLine, nCmdShow)`: четыре аргумента ровно как у `wWinMain`.

## Шаг 4. Реальная main

```bash
rea function "D:/Naben/Github/REA/samples/notepad.exe" 0x14000ad5c \
  --provider ghidra --format json
```

130 с, 1,5 МБ досье. Кратко:

| Факт | Значение |
|---|---|
| сигнатура | `__fastcall FUN_14000ad5c(HMODULE, undefined8, undefined8, uint)` |
| тело | 955 байт, 36 базовых блоков, 224 инструкции |
| вызовов | 31: 12 внутренних `FUN_*` и 19 импортов |
| цикл сообщений | `GetMessageW` → `TranslateAcceleratorW` → `IsDialogMessageW` → `TranslateMessage` → `DispatchMessageW` |
| прочее | `CoInitializeEx` / `CoUninitialize`, `SetWinEventHook` / `UnhookWinEvent`, `PostMessageW`, `HeapSetInformation` |

Это `wWinMain` блокнота: инициализация COM, акселераторы, хук WinEvent и классический
насос сообщений. Каждый пункт — именованный импорт со ссылкой из тела, а не догадка.

## Шаг 5. Класс окна и оконная процедура

```bash
rea trace "D:/Naben/Github/REA/samples/notepad.exe" "Notepad" --provider ghidra --format json
```

131 с, 16 совпадений и 14 ссылок. Среди строк, кстати, нашлись пути исходников из PDB:
`shell\osshell\accesory\notepad\notepad.cpp`, `…\filesystemhelpers.h`, `…\nprestart.cpp`.

Класс регистрируется в `FUN_14001c7bc`:

```c
ATOM FUN_14001c7bc(undefined8 param_1, HINSTANCE param_2)
{
  ...
  local_58.lpszClassName = L"Notepad";
  local_58.lpszMenuName  = (LPCWSTR)0x1;
  local_58.lpfnWndProc   = FUN_14000a280;   /* оконная процедура */
  local_58.hbrBackground = (HBRUSH)&DAT_00000006;
  local_58.hInstance     = param_2;
  return RegisterClassExW(&local_58);
}
```

Окно и текст создаются в `FUN_1400121a8`:

```c
CreateWindowExW(0, L"Notepad", L"", 0xcf0000, ...);            /* 0xcf0000 = WS_OVERLAPPEDWINDOW */
CreateWindowExW(0, L"Edit", L"", ..., hWnd, (HMENU)0xf, ...);  /* сам текст, id 0xF */
```

`FUN_14000a280` — `WndProc`, 12 493 символа псевдокода. Сообщения, по которым идёт
сравнение `param_2`:

| Код | Сообщение | Роль |
|---|---|---|
| `0x111` | `WM_COMMAND` | меню: открыть, сохранить, поиск и прочее |
| `0x10` | `WM_CLOSE` | проверка несохранённых изменений (`FUN_140009f3c`) |
| `0x2` | `WM_DESTROY` | `PostQuitMessage(0)` — конец цикла сообщений |
| `0x112` | `WM_SYSCOMMAND` | системное меню |
| `0x116` / `0x117` | `WM_INITMENUPOPUP` / `WM_MENUSELECT` | состояние пунктов меню |
| `0x20a` / `0x20e` | колесо мыши | прокрутка |
| `0x2e0` | `WM_DPICHANGED` | смена масштаба |
| `0x4e`, `0x233`, `0x16`, `0x11`, `0x319`, `0x8001` | notify, DDE, завершение сеанса, прикладные | |

Цепочка целиком:

```
entry → __scrt_common_main_seh → wWinMain
      → FUN_14001c7bc (RegisterClassExW, lpfnWndProc)
      → FUN_1400121a8 (CreateWindowExW: L"Notepad" + L"Edit")
      → FUN_14000a280 (WndProc) → WM_COMMAND → обработчики меню
```

## Шаг 6. Диспетчер команд меню

`WM_COMMAND` уходит в `FUN_140008d1c` (18 351 символ). Он получает `LOWORD(wParam)` — id
команды — и разветвляется по нему. Часть ветвей привязана к вызовам напрямую:

| id | Подпись из меню | Наблюдаемый код |
|---|---|---|
| `0x0001` | Создать | `FUN_14000fc1c` |
| `0x0002` | Открыть… | `FUN_1400086ac` |
| `0x0003` | Сохранить | `FUN_14000e808` |
| `0x0004` | Сохранить как… | `FUN_14000c670` + `CommDlgExtendedError` |
| `0x0005` | Параметры страницы… | `FUN_14000c670` |
| `0x0006` | Печать… | `SetThreadDpiAwarenessContext`, `FUN_14001e98c` |
| `0x0007` | Выход | проверка несохранённых (`FUN_140009f9c`) |
| `0x0010` | Отменить | `SendMessageW` в Edit |
| `0x0015` | Найти… (CTRL+F) | `FindTextW(&FINDREPLACEW)` |
| `0x0016` | Найти далее (F3) | тот же диалог |
| `0x0017` | Заменить… (CTRL+H) | `ReplaceTextW(&FINDREPLACEW)` |
| `0x0018` | Перейти… (CTRL+G) | `DialogBoxParamW(…, 0xE, …)` |
| `0x001c` | Поиск с помощью Bing (CTRL+E) | `ShellExecuteW` |
| `0x0021` | Шрифт… | `ChooseFontW` |
| `0x0300`–`0x0303` | Вырезать / Копировать / Вставить / Удалить | `SendMessageW` в Edit |
| `0x0040` | Просмотреть справку | `ShellExecuteW(go.microsoft.com/…)` |
| `0x0042` | Отправить отзыв | `ShellExecuteW(feedback-hub://…)` |

Детали, которые видны в коде:

- `FindTextW`/`ReplaceTextW` получают одну структуру `FINDREPLACEW`
  (`DAT_140031590`), а флаги `FR_DOWN` / `FR_MATCHCASE` / `FR_WHOLEWORD` собираются из
  состояния `DAT_1400325a0…` — то есть галочки диалога живут в глобальном состоянии процесса.
- Шрифт выбирается под `SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2)`, а размер в
  пунктах пересчитывается через `GetDeviceCaps(LOGPIXELSY)` и `MulDiv` — иначе на 150 %
  масштабе диалог показывал бы неверный кегль.

## Шаг 7. Откуда взялись подписи: MUI-ресурсы

В самом `notepad.exe` ресурсов почти нет — манифест и ссылка `Files/Resources/notepad.exe.mui`.
Меню, диалоги и строки лежат в языковом файле:

```bash
cp "C:/Windows/System32/ru-RU/notepad.exe.mui" samples/mui/
rea inspect-pe-resources "D:/Naben/Github/REA/samples/mui/notepad.exe.mui" --format json
```

**2 секунды** (движок не нужен), 16 ресурсов: меню (тип 4, id 1), четыре диалога (тип 5:
11, 12, 14, 15), строковые блоки (тип 6), акселераторы (тип 9), версия (тип 16).

Меню — плоский список из 42 записей «флаги, id, подпись». Диалоги читаются ещё проще:

| id | Заголовок |
|---|---|
| 11 | `Блокнот` |
| 12 | `Параметры страницы` |
| 14 | `Переход на строку` |
| 15 | (без заголовка) |

Гипотеза из кода подтвердилась ресурсом: `0x18` — «Перейти…», а диалог №14 — «Переход на
строку».

**Исправление собственной ошибки.** На шаге 6 я предположил, что `0x1C` — это выбор шрифта.
Меню показало: `0x1C` — «Поиск с помощью Bing», а шрифт — `0x21`. Догадка была
правдоподобной и неверной; таблица ресурсов опровергла её за две секунды. Это ровно тот
случай, ради которого Evidence отделяют от интерпретации.

## Что из этого следует

- Это штатная CRT-точка входа: сначала инициализация `/GS`-куки, затем вызов реального
  `main` через `FUN_140023d10`. Символы срезаны, поэтому Ghidra даёт `FUN_<адрес>`, а не имя.
- Вывод воспроизводим: тот же `sha256`, тот же `image_base`, то же число процедур.
- `confidence: derived` — важная деталь: псевдокод это реконструкция, а не исходник.
  Проверять гипотезы о поведении нужно трассировкой ссылок и вызовов, а не чтением одного тела.
- `unresolved_calls` — не дефект, а честный `unknown`: декодер видит инструкцию `call`,
  но цель непрямая. REA не подставляет ближайший похожий адрес и не делает вид, что
  связь установлена. Именно так и должно выглядеть «я не знаю» в машинном выводе.
- Досье на 1,5 МБ в чат не читают: сначала смотрите `procedure`, `callees`,
  `unresolved_calls` и `facet_availability`, а тело — только когда вопрос уже конкретный.
- Два независимых источника сильнее одного: код дал структуру и вызовы, ресурсы дали имена
  команд. Там, где они расходятся, и находится место для проверки — именно так была поймана
  ошибка с `0x1C`.

## Практика, которую стоит знать заранее

- Каждый вызов CLI заново импортирует бинарь: 134 с на первом `inspect`, 129 с на повторном
  с тем же `--snapshot`. Снапшот хранит **результаты**, а не базу Ghidra.
- Поэтому вопросы надо группировать: один вызов, который отвечает на несколько вопросов,
  дешевле трёх последовательных.
- Для навигации дальше: `rea function <path> <addr>`, `rea resolve-native-call-targets`,
  `rea trace-call-path` — они дают связи между процедурами, а не только тело.

## Повторить

```bash
export GHIDRA_INSTALL_DIR='C:\Users\User\workbuddy-ai\tools\ghidra_12.1.4_PUBLIC'
export JAVA_HOME='C:\Users\User\workbuddy-ai\tools\jdk-21'
rea capabilities
rea inspect "D:/Naben/Github/REA/samples/notepad.exe" --provider ghidra --format json
```
