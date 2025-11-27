package com.akslabs.circletosearch.data

sealed class SearchEngine(val displayName: String) {
    data    object Google : SearchEngine("Google")
    object Bing : SearchEngine("Bing")
    object Yandex : SearchEngine("Yandex")
    object TinEye : SearchEngine("TinEye")
    object Baidu : SearchEngine("Baidu")
    object Lenso : SearchEngine("Lenso.ai")

    companion object {
        fun values(): List<SearchEngine> = listOf(Google, Bing, Yandex, TinEye, Baidu, Lenso)
    }
    
    val name: String get() = displayName
}
