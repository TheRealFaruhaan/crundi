package mv.obitech.crundi.ui.shell

/**
 * Every area of the desktop, in the desktop's tab order, with the icon the
 * desktop gives it. Each is one tab in the shell and the root route `key`.
 *
 * `needsProject`: the area shows the current project's things (the desktop
 * scopes these tabs to the selected project). `owner` is the package that
 * builds it (see android-architecture.md): only that package changes the
 * screens; this list changes only through the Lead.
 */
enum class Area(val key: String, val label: String, val icon: String, val needsProject: Boolean, val owner: String) {
    Chats("chats", "Chats", "message-square", true, "ui.chats"),
    Terminals("terminals", "Terminals", "terminal", true, "ui.chats"),
    Git("git", "Git", "git-branch", true, "ui.workspace"),
    Files("files", "Files", "folder", true, "ui.workspace"),
    Kanban("kanban", "Kanban", "kanban", true, "ui.planning"),
    Notes("notes", "Notes", "note", true, "ui.planning"),
    MindMap("mindmap", "Mind map", "mindmap", true, "ui.planning"),
    Media("media", "Media", "images", false, "ui.workspace"),
    Factories("factories", "Factories", "factory", false, "ui.planning"),
    Schedules("schedules", "Schedules", "clock", false, "ui.planning"),
    Services("services", "Services", "server", false, "ui.operations"),
    Forwards("forwards", "Forwards", "network", false, "ui.operations"),
    Docker("docker", "Docker", "docker", false, "ui.operations"),
    Secrets("secrets", "Secrets", "lock", false, "ui.operations"),
    Usage("usage", "Usage", "bar-chart", false, "ui.operations"),
    Settings("settings", "Settings", "settings", false, "ui.operations"),
    ;

    companion object {
        fun byKey(key: String?): Area? = entries.firstOrNull { it.key == key }
    }
}
