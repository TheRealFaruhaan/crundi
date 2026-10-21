package mv.obitech.crundi.ui.shell

/**
 * Every area of the app, in the mobile web's tab order (the tab bar in
 * src/webapp-html.js), with the icon the web gives it. A [tab] is an
 * icon in the tab strip and the root route [key]; the rest are reached from
 * somewhere else (Forwards from Info and Docker, Usage from the top bar).
 * There is no Chats or Terminals tab: chats and terminals are panes in the
 * Workbench, which a project opens on.
 *
 * [global]: shown with no project chosen (the web's GLOBAL_TABS). [owner]:
 * the package that builds it (see android-architecture.md); this list
 * changes only through the Lead.
 */
enum class Area(val key: String, val label: String, val icon: String, val global: Boolean, val owner: String, val tab: Boolean = true) {
    Workbench("workbench", "Workbench", "terminal", false, "ui.workbench (Lead)"),
    Git("git", "Git", "git-branch", false, "ui.workspace"),
    Files("files", "Files", "folder", false, "ui.workspace"),
    Kanban("kanban", "Kanban", "kanban", false, "ui.planning"),
    Notes("notes", "Notes", "note", false, "ui.planning"),
    MindMap("mindmap", "Mindmap", "mindmap", true, "ui.planning"),
    Media("media", "Media", "images", true, "ui.workspace"),
    Panels("panels", "Panels", "panel", true, "ui.operations"),
    Factories("factories", "Factories", "factory", false, "ui.planning"),
    Schedules("schedules", "Schedule", "clock", false, "ui.planning"),
    Services("services", "Services", "server", false, "ui.operations"),
    Docker("docker", "Docker", "docker", true, "ui.operations"),
    Browsers("browsers", "Browsers", "globe", false, "ui.workspace"),
    Info("info", "Info", "info", true, "ui.operations"),
    Secrets("secrets", "Secrets", "lock", true, "ui.operations"),
    Settings("settings", "Settings", "settings", true, "ui.operations"),
    // Not tabs.
    Forwards("forwards", "Forwards", "network", true, "ui.operations", tab = false),
    Usage("usage", "Usage", "bar-chart", true, "ui.operations", tab = false),
    ;

    /** Old name, kept for the screens written before change 2. */
    val needsProject get() = !global

    companion object {
        fun byKey(key: String?): Area? = entries.firstOrNull { it.key == key }
        /** The tab strip: every tab, or only the global ones when no project is chosen. */
        fun tabs(hasProject: Boolean) = entries.filter { it.tab && (hasProject || it.global) }
    }
}
