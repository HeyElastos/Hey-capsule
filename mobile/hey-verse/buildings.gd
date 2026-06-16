class_name VerseBuildings
extends RefCounted

# Registry that unifies the premium VerseBuilding<Camel> modules.
#
# Each building module is a global class (class_name VerseBuilding<Camel>)
# exposing:
#   static func build() -> Node3D       # constructs the 3D building
#   static func meta()  -> Dictionary   # {"id","name","tier","rarity", ...}
#
# This registry is the single place the rest of the game (home.gd, the
# showroom, the land catalog, etc.) goes through to enumerate and build them.
#
# NOTE: the requested "manor_chateau" / VerseBuildingManorChateau module does
# not exist in the project, so it is intentionally omitted. Adding a dispatch
# arm for a class that has no definition would make this whole script fail to
# parse (unresolved identifier), breaking every consumer of VerseBuildings.
# Re-add it here (in both all() and build()) once the module ships.

# Tier ordering, lowest -> highest. all() is sorted by this.
const TIER_ORDER: Array = ["Cottage", "Villa", "Beach House", "Luxury Villa", "Penthouse", "Mansion", "Castle", "Palace"]


# Every building's meta(), ordered by tier (Cottage .. Palace).
static func all() -> Array:
	var metas: Array = [
		VerseBuildingCozyCottage.meta(),       # Cottage
		VerseBuildingTudorTownhouse.meta(),    # Villa
		VerseBuildingAlpineChalet.meta(),      # Villa
		VerseBuildingBeachHouse.meta(),        # Beach House
		VerseBuildingLuxuryVilla.meta(),       # Luxury Villa
		VerseBuildingModernVilla.meta(),       # Luxury Villa
		VerseBuildingSkyPenthouse.meta(),      # Penthouse
		VerseBuildingSkyTower.meta(),          # Penthouse
		VerseBuildingGrandMansion.meta(),      # Mansion
		VerseBuildingStoneCastle.meta(),       # Castle
		VerseBuildingDesertPalace.meta(),      # Palace
	]
	metas.sort_custom(_tier_less)
	return metas


# Stable tier comparator used by all(). Unknown tiers sort to the end; ties
# preserve their listed order so the two Villa / Luxury Villa / Penthouse
# entries keep a deterministic sequence.
static func _tier_less(a: Dictionary, b: Dictionary) -> bool:
	var ta: int = TIER_ORDER.find(str(a.get("tier", "")))
	var tb: int = TIER_ORDER.find(str(b.get("tier", "")))
	if ta == -1:
		ta = TIER_ORDER.size()
	if tb == -1:
		tb = TIER_ORDER.size()
	return ta < tb


# Build a building Node3D by id. Returns null (with a warning) on unknown id.
static func build(id: String) -> Node3D:
	match id:
		"cozy_cottage":
			return VerseBuildingCozyCottage.build()
		"tudor_townhouse":
			return VerseBuildingTudorTownhouse.build()
		"alpine_chalet":
			return VerseBuildingAlpineChalet.build()
		"beach_house":
			return VerseBuildingBeachHouse.build()
		"modern_villa":
			return VerseBuildingModernVilla.build()
		"luxury_villa":
			return VerseBuildingLuxuryVilla.build()
		"sky_penthouse":
			return VerseBuildingSkyPenthouse.build()
		"grand_mansion":
			return VerseBuildingGrandMansion.build()
		"stone_castle":
			return VerseBuildingStoneCastle.build()
		"desert_palace":
			return VerseBuildingDesertPalace.build()
		"sky_tower":
			return VerseBuildingSkyTower.build()
		_:
			push_warning("VerseBuildings.build(): unknown building id '%s'" % id)
			return null


# Rarity -> swatch colour for UI (badges, frames, marketplace tags).
static func rarity_color(r: String) -> Color:
	match r:
		"Legendary":
			return Color(1.0, 0.78, 0.22)   # gold
		"Epic":
			return Color(0.65, 0.36, 0.94)  # purple
		"Rare":
			return Color(0.27, 0.55, 0.96)  # blue
		"Uncommon":
			return Color(0.34, 0.78, 0.40)  # green
		_:
			return Color(0.62, 0.64, 0.68)  # grey
