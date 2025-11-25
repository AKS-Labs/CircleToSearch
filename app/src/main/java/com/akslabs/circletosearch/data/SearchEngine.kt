package com.akslabs.circletosearch.data

sealed class SearchEngine(val name: String) {
    object Google : SearchEngine("Google")
    object Bing : SearchEngine("Bing")
    object Yandex : SearchEngine("Yandex")
    object TinEye : SearchEngine("TinEye")
    object Baidu : SearchEngine("Baidu")

    companion object {
        fun getAll() = listOf(Google, Bing, Yandex, TinEye, Baidu)
    }
}
