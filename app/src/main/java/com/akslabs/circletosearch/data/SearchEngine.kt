package com.akslabs.circletosearch.data

sealed class SearchEngine(val name: String, val queryUrl: String) {
    // Using generic search queries for simulation since we can't upload the image directly in this demo
    object Google : SearchEngine("Google", "https://www.google.com/search?q=Circle+to+Search+Demo&tbm=isch")
    object Bing : SearchEngine("Bing", "https://www.bing.com/images/search?q=Circle+to+Search+Demo")
    object DuckDuckGo : SearchEngine("DuckDuckGo", "https://duckduckgo.com/?q=Circle+to+Search+Demo&iax=images&ia=images")
    object Yandex : SearchEngine("Yandex", "https://yandex.com/images/search?text=Circle+to+Search+Demo")

    companion object {
        fun getAll() = listOf(Google, Bing, DuckDuckGo, Yandex)
    }
}
