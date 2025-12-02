package com.akslabs.circletosearch.data

sealed class SearchEngine(val displayName: String) {
    object Google : SearchEngine("Google")
    object Bing : SearchEngine("Bing")
    object Yandex : SearchEngine("Yandex")
    object TinEye : SearchEngine("TinEye")
    object Perplexity : SearchEngine("Perplexity")
    object ChatGPT : SearchEngine("ChatGPT")

    companion object {
        fun values(): List<SearchEngine> = listOf(Google, Bing, Yandex, TinEye, Perplexity, ChatGPT)
    }
    
    val name: String get() = displayName
}

val SearchEngine.isDirectUpload: Boolean
    get() = when (this) {
        SearchEngine.Perplexity, SearchEngine.ChatGPT -> true
        else -> false
    }
