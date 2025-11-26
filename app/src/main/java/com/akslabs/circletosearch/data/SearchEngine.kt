package com.akslabs.circletosearch.data

sealed class SearchEngine(val displayName: String) {
    data    object Google : SearchEngine("Google")
    object Bing : SearchEngine("Bing")
    object Yandex : SearchEngine("Yandex")
    object Yahoo : SearchEngine("Yahoo")
    object TinEye : SearchEngine("TinEye")
    object Baidu : SearchEngine("Baidu")
    object SauceNAO : SearchEngine("SauceNAO")
    object IQDB : SearchEngine("IQDB")
    object ASCII2D : SearchEngine("ASCII2D")
    object Lenso : SearchEngine("Lenso.ai")

    companion object {
        fun values(): List<SearchEngine> = listOf(Google, Bing, Yandex, Yahoo, TinEye, Baidu, SauceNAO, IQDB, ASCII2D, Lenso)
    }
    
    val name: String get() = displayName
}
