package dev.artwindow

/**
 * One of the four museums `catalogue/build.py` draws paintings from. [code] is the
 * TSV's `source` column and the prefix a download's filename carries — [keyOf] in
 * Museums.kt reads it back out to recognise this app's own work, so a code can never
 * change once paintings carrying it exist on a phone.
 */
enum class MuseumSource(val code: String, val displayName: String, val shortName: String) {
    MET("met", "The Metropolitan Museum of Art", "the Met"),
    NGA("nga", "National Gallery of Art, Washington", "the NGA"),
    CMA("cma", "Cleveland Museum of Art", "Cleveland"),
    SMK("smk", "SMK – National Gallery of Denmark", "SMK"),
    WMC("wmc", "Wikimedia Commons", "Wikimedia Commons");

    companion object {
        fun byCode(code: String): MuseumSource? = entries.firstOrNull { it.code == code }
    }
}
