### Как просить использовать GUI Tester в новых сессиях

В корне проекта настроен файл [.mcp.json](file:///d:/Naben/Github/rust/Aquilum-NP/.mcp.json), регистрирующий сервер `gui-tester`:

```json
{
  "mcpServers": {
    "gui-tester": {
      "command": "python",
      "args": ["tools/gui_tester/mcp_server.py"],
      "env": { "GUI_TESTER_WINDOW": "Vibecodium" }
    }
  }
}
```

#### Примеры запросов для агента:

* **Запуск и проверка экрана:**
  > *"Запусти Vibecodium через MCP gui-tester и покажи скриншот окна с координатной сеткой."*
* **Проверка конкретного элемента или действия:**
  > *"Используй MCP gui-tester: открой заметку X, нажми на кнопку Y и проверь, появилось ли меню."*
* **Визуальная инспекция области:**
  > *"Сделай зум области таблицы в окне Vibecodium через MCP `zoom` и скопируй скриншот в буфер обмена."*
* **Регрессионное тестирование:**
  > *"Сделай визуальный diff текущего экрана с базовым шаблоном через `compare_to_baseline`."*

> [!TIP]
> **Автономный фоллбек**: Если сессия запущена в среде, где MCP-клиент не подключен напрямую к диалогу, в корневом [AGENTS.md](file:///d:/Naben/Github/rust/Aquilum-NP/AGENTS.md) и [AI_GUIDE.md](file:///d:/Naben/Github/rust/Aquilum-NP/tools/gui_tester/AI_GUIDE.md) уже прописана инструкция для агентов. Вы можете просто написать:
> *"Протестируй интерфейс Vibecodium через `tools/gui_tester`"* — агент автоматически вызовет либо методы MCP-сервера, либо CLI `python tools/gui_tester/cli.py`.