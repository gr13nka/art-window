package dev.artwindow

/**
 * One of the museums `catalogue/build.py` draws paintings from. [code] is the
 * TSV's `source` column and the prefix a download's filename carries — [keyOf] in
 * Museums.kt reads it back out to recognise this app's own work, so a code can never
 * change once paintings carrying it exist on a phone. That is also why [WMC] is
 * still here: the catalogue no longer draws from Wikimedia Commons, which warrants
 * no licence per file, but a Commons picture already on a phone is still this
 * app's own by its file name.
 */
enum class MuseumSource(val code: String, val displayName: String, val shortName: String) {
    MET("met", "The Metropolitan Museum of Art", "the Met"),
    NGA("nga", "National Gallery of Art, Washington", "the NGA"),
    CMA("cma", "Cleveland Museum of Art", "Cleveland"),
    SMK("smk", "SMK – National Gallery of Denmark", "SMK"),
    RIJKS("rijks", "Rijksmuseum, Amsterdam", "the Rijksmuseum"),
    GETTY("getty", "J. Paul Getty Museum", "the Getty"),
    WMC("wmc", "Wikimedia Commons", "Wikimedia Commons");

    companion object {
        fun byCode(code: String): MuseumSource? = entries.firstOrNull { it.code == code }
    }
}
