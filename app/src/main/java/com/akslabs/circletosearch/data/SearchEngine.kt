package com.akslabs.circletosearch.data

sealed class SearchEngine(val name: String, val queryUrl: String) {
    object Google : SearchEngine("Google", "https://www.google.com/searchbyimage?image_url=")
    object Bing : SearchEngine("Bing", "https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP&q=imgurl:")
    object DuckDuckGo : SearchEngine("DuckDuckGo", "https://duckduckgo.com/?q=") // DDG doesn't have a direct public image search URL API easily accessible like this without upload, but we'll use a placeholder
    object Yandex : SearchEngine("Yandex", "https://yandex.com/images/search?rpt=imageview&url=")

    companion object {
        fun getAll() = listOf(Google, Bing, DuckDuckGo, Yandex)
    }
}
