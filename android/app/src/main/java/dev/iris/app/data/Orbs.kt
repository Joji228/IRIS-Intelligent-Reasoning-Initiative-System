package dev.iris.app.data

// What an agent's orb shows: its state (what it's up to) and what its speech bubble says
// (app/src/lib/orbs.ts).

/** Ten minutes without a touch, or that long in the background: idle orbs doze. */
const val SLEEP_AFTER = 10 * 60_000L

enum class OrbState {
    Idle, Sleeping, Thinking, Searching, Reading, Working, Team, Reviewing, Waiting, Designing,
    Celebrate, Failed, Unread, Spawn, Spin;

    /** Doing something (rings spin, a chip shows the step). */
    val busy get() = this !in setOf(Idle, Sleeping, Unread, Celebrate, Failed, Spawn)
}

private val SEARCH = setOf(
    "web_search", "web_fetch", "search_chats", "code_search", "search_apps", "browser_open", "browser_click",
    "browser_type", "browser_scroll", "browser_back", "browser_login",
)
private val READ = setOf(
    "read_file", "list_files", "code_read", "code_diff", "browser_read", "view_image", "use_skill", "get_settings",
    "list_routines", "list_accounts", "list_models", "get_time", "system_status", "code_status",
)
private val TEAM = setOf("list_agents", "create_agent", "delegate_task", "check_task", "dismiss_agent", "keep_agent", "request_tools")

/** What the orb is up to, from the agent's run, mood and unread replies. */
fun orbState(
    activity: String? = null,
    mood: String? = null,
    working: Boolean = false,
    waiting: Boolean = false,
    unread: Boolean = false,
    sleepy: Boolean = false,
): OrbState {
    when (mood) {
        "spawn" -> return OrbState.Spawn
        "spin" -> return OrbState.Spin
        "happy" -> return OrbState.Celebrate
        "sad" -> return OrbState.Failed
    }
    if (waiting || activity == "waiting") return OrbState.Waiting
    if (activity != null || working) {
        val a = activity ?: "thinking"
        return when {
            a == "thinking" || a == "compacting" -> OrbState.Thinking
            a == "reviewing" -> OrbState.Reviewing
            a == "designing" -> OrbState.Designing
            a in TEAM -> OrbState.Team
            a in SEARCH || a.startsWith("browser_") -> OrbState.Searching
            a in READ -> OrbState.Reading
            else -> OrbState.Working
        }
    }
    if (unread) return OrbState.Unread
    if (sleepy) return OrbState.Sleeping
    return OrbState.Idle
}

private val BY_TOOL = mapOf(
    "web_search" to listOf("Searching the web…", "Looking it up…", "Going through results…"),
    "web_fetch" to listOf("Reading a page…", "Skimming the page…"),
    "search_chats" to listOf("Searching past chats…", "Jogging my memory…"),
    "read_file" to listOf("Reading files…", "Looking it over…"),
    "list_files" to listOf("Looking at files…"),
    "write_file" to listOf("Writing it down…", "Saving a file…"),
    "run_command" to listOf("Running a command…", "Checking the output…"),
    "view_image" to listOf("Looking at the picture…"),
    "memory" to listOf("Making a note…"),
    "get_time" to listOf("Checking the time…"),
    "system_status" to listOf("Checking the server…", "Taking its pulse…"),
    "app_updates" to listOf("Checking for updates…"),
    "search_apps" to listOf("Looking for the app…"),
    "update_apps" to listOf("Updating apps…"),
    "install_app" to listOf("Installing an app…"),
    "uninstall_app" to listOf("Removing an app…"),
    "windows_updates" to listOf("Checking Windows Update…"),
    "power" to listOf("Getting ready to restart…"),
    "list_agents" to listOf("Checking the team…"),
    "create_agent" to listOf("Bringing in a new agent…"),
    "delegate_task" to listOf("Handing out a task…", "Passing it along…"),
    "check_task" to listOf("Checking on the team…"),
    "dismiss_agent" to listOf("Wrapping up with an agent…"),
    "keep_agent" to listOf("Keeping an agent on…"),
    "request_tools" to listOf("Asking Iris for tools…"),
    "create_routine" to listOf("Scheduling a routine…"),
    "list_routines" to listOf("Checking the routines…"),
    "delete_routine" to listOf("Removing a routine…"),
    "get_settings" to listOf("Checking the settings…"),
    "change_setting" to listOf("Changing a setting…"),
    "list_models" to listOf("Checking the models…"),
    "switch_model" to listOf("Switching models…"),
    "use_skill" to listOf("Reading a how-to…"),
    "save_skill" to listOf("Saving a how-to…"),
    "list_accounts" to listOf("Checking saved accounts…"),
    "browser_open" to listOf("Opening a page…", "Browsing…"),
    "browser_read" to listOf("Reading the page…"),
    "browser_click" to listOf("Clicking through…"),
    "browser_type" to listOf("Typing…"),
    "browser_scroll" to listOf("Scrolling…"),
    "browser_login" to listOf("Signing in…"),
    "code_status" to listOf("Checking the code…"),
    "code_search" to listOf("Searching the code…"),
    "code_read" to listOf("Reading the code…"),
    "code_edit" to listOf("Editing the code…", "Making changes…"),
    "code_write" to listOf("Writing code…"),
    "code_diff" to listOf("Looking at the changes…"),
    "code_apply" to listOf("Testing the change…"),
    "code_install" to listOf("Building the new version…"),
    "code_verify" to listOf("Checking the new version…"),
    "compacting" to listOf("Tidying up the chat…"),
    "designing" to listOf("Getting to know my job…", "Setting myself up…"),
    "reviewing" to listOf("Double-checking that command…"),
    "ask_user" to listOf("Asking you something…"),
)

/** Plain words for the speech bubble, a few to rotate through on long steps. */
fun phrases(state: OrbState, activity: String?): List<String> {
    val a = activity.orEmpty()
    return when (state) {
        OrbState.Waiting -> listOf("Waiting for you…", "Need your OK…")
        OrbState.Celebrate -> listOf("Done!", "All done!")
        OrbState.Failed -> listOf("Hit a snag…")
        OrbState.Unread -> listOf("New reply!")
        OrbState.Spawn -> listOf("Hi!")
        OrbState.Sleeping -> listOf("Zzz…")
        OrbState.Idle -> emptyList()
        OrbState.Thinking ->
            if (a == "compacting") BY_TOOL.getValue("compacting")
            else listOf("Thinking…", "Mulling it over…", "Piecing it together…", "Working it out…")
        else -> BY_TOOL[a]
            ?: if (a.startsWith("mcp__")) listOf("Using ${a.removePrefix("mcp__").substringBefore("__")}…")
            else listOf("Working on it…", "Chipping away…")
    }
}
