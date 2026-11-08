package dev.iris.app.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.scale
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.iris.app.data.ABILITY_GROUPS
import dev.iris.app.data.AGENT_COLORS
import dev.iris.app.data.Agent
import dev.iris.app.data.BUILT_IN_ABILITIES
import dev.iris.app.data.IrisViewModel
import dev.iris.app.data.MemoryText
import dev.iris.app.data.REASONING_LEVELS
import dev.iris.app.data.modelName
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray

/** One agent's settings (AgentEditor.svelte): saved together with Save. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AgentEditScreen(vm: IrisViewModel, agent: Agent) {
    val scope = rememberCoroutineScope()
    val overlays = LocalOverlays.current
    val main = agent.role == "main"
    var name by remember(agent.id) { mutableStateOf(agent.name) }
    var description by remember(agent.id) { mutableStateOf(agent.description) }
    var color by remember(agent.id) { mutableStateOf(agent.color) }
    var prompt by remember(agent.id) { mutableStateOf(agent.systemPrompt) }
    var reasoning by remember(agent.id) { mutableStateOf(agent.reasoningEffort) }
    var auto by remember(agent.id) { mutableStateOf(if (!agent.autoApprove) "off" else if (agent.autoFull) "full" else "careful") }
    var tools by remember(agent.id) { mutableStateOf(agent.tools) }
    var provider by remember(agent.id) { mutableStateOf(agent.provider) }
    var model by remember(agent.id) { mutableStateOf(agent.model) }
    var memory by remember(agent.id) { mutableStateOf<MemoryText?>(null) }
    var memoryText by remember(agent.id) { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(agent.id) {
        vm.attempt { agentMemory(agent.id) }?.let { memory = it; memoryText = it.text }
        if (vm.providers.isEmpty()) vm.loadModelSettings()
    }

    val dirty = name != agent.name || description != agent.description || color != agent.color || prompt != agent.systemPrompt ||
        reasoning != agent.reasoningEffort || tools != agent.tools || provider != agent.provider || model != agent.model ||
        auto != (if (!agent.autoApprove) "off" else if (agent.autoFull) "full" else "careful") ||
        (memory != null && memoryText != memory?.text)

    fun save() {
        busy = true
        scope.launch {
            val patch = buildJsonObject {
                if (name != agent.name) put("name", name.trim())
                if (description != agent.description) put("description", description.trim())
                if (color != agent.color) put("color", color)
                if (prompt != agent.systemPrompt) put("system_prompt", prompt)
                if (reasoning != agent.reasoningEffort) put("reasoning_effort", reasoning)
                if (provider != agent.provider || model != agent.model) { put("provider", provider); put("model", model) }
                if (tools != agent.tools) putJsonArray("tools") { tools.forEach { add(JsonPrimitive(it)) } }
                if (auto != (if (!agent.autoApprove) "off" else if (agent.autoFull) "full" else "careful")) {
                    put("auto_approve", auto != "off")
                    put("auto_full", auto == "full")
                }
            }
            val saved = vm.attempt { patchAgent(agent.id, patch) }
            if (saved != null) vm.agentSaved(saved)
            val m = memory
            val memOk = if (m != null && memoryText != m.text) vm.attempt { putAgentMemory(agent.id, memoryText) }?.also { memory = it } != null else true
            busy = false
            if (saved != null && memOk) {
                vm.showToast("${saved.name} saved")
                vm.back()
            }
        }
    }

    fun pickModel() {
        val default = vm.defaultModel
        val defaultLabel = "Default · ${default?.let { modelName(it.model) } ?: "from Settings"}"
        fun chooseModel(p: String) = scope.launch {
            val list = vm.attempt { models(p) }.orEmpty()
            overlays.picker("Model", listOf("" to defaultLabel) + list.map { it to modelName(it) }, model.ifEmpty { "" }) {
                if (it.isEmpty()) { provider = ""; model = "" } else { provider = p; model = it }
            }
        }
        if (vm.chatProviders.size > 1) {
            overlays.picker("Provider", listOf("" to defaultLabel) + vm.chatProviders.map { it.id to it.name }, provider) {
                if (it.isEmpty()) { provider = ""; model = "" } else chooseModel(it)
            }
        } else chooseModel(provider.ifEmpty { default?.provider ?: vm.chatProviders.firstOrNull()?.id.orEmpty() })
    }

    Page(agent.name, { vm.back() }, subtitle = "Agent settings", action = {
        Box(Modifier.padding(end = 8.dp)) { Button("Save", ::save, enabled = dirty && name.isNotBlank(), busy = busy) }
    }) {
        item {
            Card {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Orb(agent.copy(color = color), 52.dp)
                    Column(Modifier.weight(1f)) { Label("Name"); Field(name, { name = it }) }
                }
                Column { Label("Description"); Field(description, { description = it }, singleLine = false) }
                Column {
                    Label("Color")
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        for (c in AGENT_COLORS) {
                            val on = c.equals(color, true)
                            val s by animateFloatAsState(if (on) 1.12f else 1f, label = "swatch")
                            Box(
                                Modifier.size(30.dp).scale(s).background(parseColor(c), CircleShape)
                                    .border(2.dp, if (on) C.text else C.bg.copy(alpha = 0f), CircleShape)
                                    .press { color = c },
                            )
                        }
                    }
                }
            }
        }
        item {
            Card {
                SelectField("Model", if (model.isEmpty()) "Default · ${vm.defaultModel?.let { modelName(it.model) } ?: "from Settings"}" else "${modelName(model)} · ${vm.providers.firstOrNull { it.id == provider }?.name ?: provider}", ::pickModel)
                Hint(if (provider.isNotEmpty()) "This agent always uses this model." else "Follows the default model from Settings → Models.")
                Divider()
                Label("Reasoning effort")
                Chips(listOf("" to "Default") + REASONING_LEVELS, reasoning) { reasoning = it }
                Divider()
                Label("Auto mode")
                ChoiceCards(AUTO_MODES, auto) { auto = it }
            }
        }
        item {
            Card {
                CardTitle("Abilities")
                if (main) {
                    Hint("Iris has every ability: $BUILT_IN_ABILITIES, plus files and computer control. Risky steps still ask you.")
                } else {
                    for (g in ABILITY_GROUPS) {
                        val on = g.tools.all { it in tools }
                        SwitchRow(g.name, g.description, on, { want ->
                            tools = if (want) (tools + g.tools).distinct() else tools - g.tools.toSet()
                        }, badge = if (g.asks) ({ Badge("Asks first", C.warn, Lucide.ShieldAlert) }) else null)
                    }
                    Hint("Always on for every agent: $BUILT_IN_ABILITIES. If it needs more, it asks Iris, who grants what fits.")
                }
            }
        }
        item {
            Card {
                CardTitle("Personality & instructions", "SOUL.md: who this agent is and how it works.")
                TextArea(prompt, { prompt = it }, "Describe who this agent is, how it should behave and what it should focus on.", minHeight = 160, maxHeight = 420)
            }
        }
        item {
            val m = memory
            Card {
                CardTitle("Long-term memory", "MEMORY.md: notes ${agent.name} keeps across chats and compactions. It adds to them itself.")
                TextArea(memoryText, { memoryText = it }, "Nothing yet.", minHeight = 110, mono = true, maxLength = m?.limit)
                Txt("${memoryText.trim().length} / ${m?.limit ?: "…"}", 12.sp, C.faint)
            }
        }
        item {
            Button("Save", ::save, Modifier.fillMaxWidth(), enabled = dirty && name.isNotBlank(), busy = busy)
        }
    }
}
