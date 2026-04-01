package com.akslabs.circletosearch.data

import com.akslabs.circletosearch.ui.components.TextNode
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

object AssistDataRepository {
    private val _assistNodes = MutableStateFlow<List<TextNode>>(emptyList())
    val assistNodes: StateFlow<List<TextNode>> = _assistNodes.asStateFlow()

    private val _isDataReady = MutableStateFlow(false)
    val isDataReady: StateFlow<Boolean> = _isDataReady.asStateFlow()

    fun setNodes(nodes: List<TextNode>) {
        _assistNodes.value = nodes
        _isDataReady.value = true
    }

    fun getNodes(): List<TextNode> {
        return _assistNodes.value
    }

    fun clear() {
        _assistNodes.value = emptyList()
        _isDataReady.value = false
    }
}
