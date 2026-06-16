class_name VerseCatalog
extends RefCounted
## The premium Hey Verse showroom catalog — the single browsable, sellable
## index over every object module (catalog_hats / catalog_seating /
## catalog_tables / catalog_lighting / catalog_wallart / catalog_plants /
## catalog_decor).
##
## `all()` returns the FULL NFT-trait record for every item:
##   {
##     id          unique builder id ("crown", "royal_throne", ...)
##     name        marketplace display name
##     kind        "hat" | "seating" | "table" | "lighting" | "wallart"
##                 | "plant" | "decor"   (used for showroom row grouping + filters)
##     theme       the module group this item lives in ("hats", "seating", ...)
##     rarity      "Common" | "Uncommon" | "Rare" | "Epic" | "Legendary"
##     description marketplace blurb
##     attributes  Array of {trait_type, value} — the filterable NFT traits
##   }
## This record IS the on-chain metadata shape: a minted item is the same
## Dictionary with the ddrm/token fields (see items.gd) filled in.
##
## `build(id)` dispatches to the owning module's static builder by id and
## returns a fresh Node3D (caller owns it; add_child + position it). Each
## module is referenced by its class_name so the .godot class cache resolves
## them at parse time — no res:// path coupling.

## RARITY_COLOR — the canonical tint for each rarity, used by the gallery to
## color-code labels (and reusable anywhere a rarity needs a swatch).
const RARITY_COLOR := {
	"Common": Color(0.78, 0.80, 0.84),      # cool grey-white
	"Uncommon": Color(0.42, 0.84, 0.46),    # green
	"Rare": Color(0.36, 0.62, 0.98),        # blue
	"Epic": Color(0.74, 0.42, 0.96),        # purple
	"Legendary": Color(1.00, 0.74, 0.22),   # gold
}

## The kind -> module class map, so build() can dispatch generically and the
## gallery can group rows by kind in a stable order.
const KIND_ORDER := ["hat", "seating", "table", "lighting", "wallart", "plant", "decor"]


## Every showroom item with its full NFT-trait record. Order is grouped by
## module (hats -> seating -> tables -> lighting -> wallart -> plants -> decor)
## so the gallery lays out tidy per-kind rows by simply walking this list.
static func all() -> Array:
	var items: Array = []

	# ---- HATS (VerseCatalogHats) ----
	items.append(_rec("party_hat", "Confetti Cone Deluxe", "hat", "hats", "Common",
		"A candy-striped lacquer party cone topped with a glowing star and a burst of floating confetti that never lands.",
		[["Material", "Glossy Lacquer"], ["Style", "Birthday Cone"], ["Color", "Sky Blue & Candy Stripe"], ["Accent", "Cream Pompom"], ["Glow", "Star Topper & Confetti"], ["Vibe", "Celebration"]]))
	items.append(_rec("top_hat", "Midnight Dapper Topper", "hat", "hats", "Uncommon",
		"A tall midnight-felt top hat with crimson satin, a brass buckle, a pearl stick-pin, and a tucked silk rose for the discerning gentlebot.",
		[["Material", "Midnight Felt & Satin"], ["Style", "Victorian Formal"], ["Trim", "Brass Buckle"], ["Accent", "Silk Rose & Pearl Pin"], ["Color", "Midnight & Crimson"], ["Vibe", "Dapper"]]))
	items.append(_rec("propeller_cap", "Whirlybird Beanie", "hat", "hats", "Uncommon",
		"A stitched four-colour gore-panel beanie crowned with a real chrome-post propeller caught mid-spin.",
		[["Material", "Glossy Lacquer & Chrome"], ["Style", "Playground Classic"], ["Color", "Primary Rainbow"], ["Feature", "Spinnable Propeller"], ["Detail", "Stitched Gore Panels"], ["Vibe", "Playful"]]))
	items.append(_rec("cat_ears", "Kawaii Bell Ears", "hat", "hats", "Uncommon",
		"A glossy headband with plush two-tone cat ears, dainty whiskers, a pink crest sparkle, and gold bell charms that beg to be booped.",
		[["Material", "Plush Felt & Gloss"], ["Style", "Kawaii Neko"], ["Color", "Charcoal & Blush Pink"], ["Charm", "Gold Bells on Chains"], ["Accent", "Pink Sparkle Gem"], ["Detail", "Whiskers"], ["Vibe", "Adorable"]]))
	items.append(_rec("viking_helmet", "Forged Frosthorn Helm", "hat", "hats", "Rare",
		"A brushed-iron war helm with a riveted brass brow band, a forged nose guard, a ruby finial, and a pair of banded bone horns.",
		[["Material", "Brushed Iron & Brass"], ["Style", "Norse Warrior"], ["Horns", "Banded Bone"], ["Gem", "Ruby Finial"], ["Color", "Iron, Brass & Bone"], ["Vibe", "Heroic"]]))
	items.append(_rec("headphones", "Studio Glow Cans", "hat", "hats", "Rare",
		"Audiophile studio headphones with a chrome slider band, plush memory-foam cups ringed in glowing cyan, a boom mic, and live level LEDs.",
		[["Material", "Piano-Black Gloss & Chrome"], ["Style", "Studio Audiophile"], ["Glow", "Cyan RGB Rings"], ["Feature", "Boom Mic"], ["Detail", "Level LEDs"], ["Vibe", "On Air"]]))
	items.append(_rec("flower_crown", "Wildbloom Coronet", "hat", "hats", "Rare",
		"A woven vine crown bursting with layered multi-petal blossoms, glowing pollen hearts, ruby berries, and trailing ivy.",
		[["Material", "Woven Vine & Glossy Petal"], ["Style", "Cottagecore"], ["Color", "Wildflower Pastels"], ["Glow", "Pollen Hearts"], ["Detail", "Berries & Trailing Ivy"], ["Vibe", "Springtime"]]))
	items.append(_rec("wizard_hat", "Starweaver's Cap", "hat", "hats", "Epic",
		"A tall droopy indigo wizard hat dusted with emissive stars and a crescent moon, ringed by a gold band of bezel-set gems, with sparkles in orbit.",
		[["Material", "Indigo Felt & Gold"], ["Style", "Arcane Sorcerer"], ["Color", "Deep Indigo"], ["Glow", "Stars, Moon & Gems"], ["Gem", "Bezel-Set Sapphire Band"], ["Detail", "Orbiting Sparkles"], ["Vibe", "Mystical"]]))
	items.append(_rec("crown", "Sovereign's Jewel Crown", "hat", "hats", "Epic",
		"A regal polished-gold crown of five fleur points crowned with faceted ruby, sapphire, and emerald jewels, bezel-set friezes, and pearl beading.",
		[["Material", "Polished Gold"], ["Style", "Royal Regalia"], ["Gem", "Ruby, Sapphire & Emerald"], ["Detail", "Pearl Beading & Bezel Frieze"], ["Glow", "Radiant Jewels"], ["Vibe", "Majestic"]]))
	items.append(_rec("halo", "Seraph's Winged Halo", "hat", "hats", "Epic",
		"A floating forged-gold halo of scrollwork and bezel-set sapphires, flanked by a pair of layered celestial feather wings and a radiant core.",
		[["Material", "Polished Gold & Pearl Feather"], ["Style", "Celestial Seraph"], ["Wings", "Layered Feather Pair"], ["Gem", "Bezel-Set Sapphires"], ["Glow", "Radiant Halo Core"], ["Detail", "Gold Fleur Points & Twinkle Stars"], ["Vibe", "Angelic"]]))
	items.append(_rec("astronaut_helmet", "Orbit Pioneer Helm", "hat", "hats", "Legendary",
		"A glossy spacesuit helmet with a clear tinted glass dome over a glowing cyan visor, a gold sun-shield, head lamps, hose ports, and a lit HUD reticle.",
		[["Material", "Glossy Shell, Glass & Brushed Metal"], ["Style", "Spacefarer"], ["Feature", "Clear Glass Dome"], ["Glow", "Cyan Visor, Lamps & HUD"], ["Trim", "Gold Sun-Shield"], ["Detail", "Antenna & Hose Ports"], ["Vibe", "Cosmic Hero"]]))
	items.append(_rec("flame_crown", "Emberlord's Blaze Crown", "hat", "hats", "Legendary",
		"A blackened-gold crown with molten ember seams, wreathed in living flame tongues that burn red to white-hot, a glowing gem heart, and drifting embers.",
		[["Material", "Blackened Gold & Living Flame"], ["Style", "Infernal Sovereign"], ["Glow", "Red-to-White Fire"], ["Gem", "Molten Heart"], ["Detail", "Ember Motes & Heat Haze"], ["Color", "Charcoal & Inferno"], ["Vibe", "Blazing"]]))

	# ---- SEATING (VerseCatalogSeating) ----
	items.append(_rec("velvet_armchair", "Periwinkle Plush Armchair", "seating", "seating", "Uncommon",
		"A button-tufted periwinkle velvet armchair with brass nailhead trim, curling wing arms and gold-piped cushions on turned wood legs.",
		[["Material", "Velvet & Brass"], ["Style", "Cozy Classic"], ["Color", "Periwinkle"], ["Detail", "Button Tufting"], ["Trim", "Brass Nailhead"], ["Vibe", "Warm & Inviting"]]))
	items.append(_rec("royal_throne", "Crown Sovereign Throne", "seating", "seating", "Legendary",
		"A towering crimson-velvet throne on a stepped gold dais, crowned with a spiked gold crest, faceted glowing gemstones, lion-paw feet and a luminous central crown jewel.",
		[["Material", "Crimson Velvet & Solid Gold"], ["Style", "Royal Baroque"], ["Color", "Crimson & Gold"], ["Gemstones", "Faceted & Glowing"], ["Feet", "Lion Paw"], ["Vibe", "Maximum Opulence"]]))
	items.append(_rec("gaming_chair", "Neon Apex Racer", "seating", "seating", "Rare",
		"A glossy carbon racing bucket seat with cyan-and-magenta neon trim, a glowing winged emblem, headrest and lumbar pillows, a fold-out footrest and a 5-star base lit with full RGB underglow.",
		[["Material", "Glossy Carbon & Chrome"], ["Style", "Esports Racer"], ["Color", "Carbon Black"], ["Lighting", "RGB Underglow"], ["Accent", "Cyan & Magenta Neon"], ["Vibe", "High-Octane"]]))
	items.append(_rec("beanbag", "Sunset Slouch Beanbag", "seating", "seating", "Common",
		"A slouchy two-tone mustard-and-pumpkin beanbag with stitched gore seams, a drawcord top knot, a little leather brand patch and a deep sink-in dimple.",
		[["Material", "Canvas Fabric"], ["Style", "Casual Lounge"], ["Color", "Mustard & Pumpkin"], ["Detail", "Leather Brand Patch"], ["Vibe", "Ultra Comfy"]]))
	items.append(_rec("carved_dining_chair", "Heritage Walnut Chair", "seating", "seating", "Uncommon",
		"A heritage walnut dining chair with a turned spindle back, a ribbon-carved splat, a gilt crest medallion, fluted legs with stretchers and a red damask piped cushion.",
		[["Material", "Walnut & Damask"], ["Style", "Heritage Carved"], ["Color", "Warm Walnut"], ["Detail", "Gilt Medallion"], ["Back", "Ribbon Splat"], ["Vibe", "Old-World Charm"]]))
	items.append(_rec("mushroom_stool", "Toadstool Fairy Seat", "seating", "seating", "Rare",
		"A whimsical candy-red toadstool stool with cream spots, softly glowing gills underneath, a chubby cream stalk, a tiny ladybug rider and a glowing fairy-ring moss base.",
		[["Material", "Glossy Ceramic"], ["Style", "Storybook Whimsy"], ["Color", "Candy Red & Cream"], ["Glow", "Bioluminescent Gills"], ["Easter Egg", "Ladybug Rider"], ["Vibe", "Enchanted Forest"]]))
	items.append(_rec("swing_seat", "Honey Rattan Egg Swing", "seating", "seating", "Epic",
		"A hanging honey-rattan egg chair on a gunmetal gooseneck stand, with a deep seafoam cushion, a tasselled coral pillow, trailing ivy and a draped string of warm fairy lights.",
		[["Material", "Woven Rattan & Gunmetal"], ["Style", "Boho Hanging"], ["Color", "Honey & Seafoam"], ["Lighting", "Warm Fairy Lights"], ["Foliage", "Trailing Ivy"], ["Vibe", "Dreamy Retreat"]]))
	items.append(_rec("chesterfield_sofa", "Oxblood Chesterfield", "seating", "seating", "Epic",
		"A deep button-tufted oxblood-leather Chesterfield with low rolled arms, double rows of brass studs, a folded cream throw, teal accent pillows and turned wood bun feet.",
		[["Material", "Oxblood Leather & Brass"], ["Style", "Gentleman's Club"], ["Color", "Oxblood"], ["Detail", "Deep Button Tufting"], ["Trim", "Brass Stud Rows"], ["Vibe", "Distinguished Luxury"]]))
	items.append(_rec("hammock", "Seaside Stripe Hammock", "seating", "seating", "Rare",
		"A striped teal-and-coral canvas hammock slung between two A-frame wooden posts, with rope fans, a fringed edge, a sunny throw pillow and a swag of warm fairy lights between the apexes.",
		[["Material", "Striped Canvas & Wood"], ["Style", "Seaside Resort"], ["Color", "Teal & Coral"], ["Lighting", "String Fairy Lights"], ["Detail", "Fringed Edge"], ["Vibe", "Lazy Afternoon"]]))
	items.append(_rec("cloud_sofa", "Dreamcloud Sofa", "seating", "seating", "Legendary",
		"An oversized sofa sculpted from puffy pastel cloud lobes with a sky-blue gradient underside, a soft glow rim, a rising rainbow arc and drifting golden star sparkles.",
		[["Material", "Plush Cloud Fabric"], ["Style", "Surreal Dreamscape"], ["Color", "Pastel Sky"], ["Glow", "Soft Rim & Star Sparkles"], ["Feature", "Rainbow Arc"], ["Vibe", "Pure Bliss"]]))

	# ---- TABLES (VerseCatalogTables) ----
	items.append(_rec("marble_coffee_table", "Veined Marble Lounge Table", "table", "tables", "Rare",
		"A beveled veined-marble slab floats on a brass cross-frame, crowned with a turned candlestick, a fruit dish, and a single glowing gem.",
		[["Material", "White Marble & Brass"], ["Style", "Mid-Century Luxe"], ["Surface", "Smoked Glass Shelf"], ["Accent", "Gilded Vein"], ["Vibe", "Lounge Centerpiece"], ["Glow", "Candle Flame"]]))
	items.append(_rec("oak_dining_table", "Honest Oak Farmhouse Table", "table", "tables", "Uncommon",
		"A big planked-oak farmhouse table with iron-strapped chamfered legs, a linen runner, a ceramic fruit bowl, and a warm candle pair.",
		[["Material", "Solid Oak & Forged Iron"], ["Style", "Cottage Farmhouse"], ["Top", "Planked w/ Breadboard Ends"], ["Seats", "Six"], ["Theme", "Family Dinner"], ["Vibe", "Cozy & Honest"]]))
	items.append(_rec("glowing_desk", "Neon Battlestation Desk", "table", "tables", "Epic",
		"A graphite cyber-desk with RGB edge grooves, a glowing monitor, a fan-lit tower, a headphone stand, and a tiny glowing desk plant.",
		[["Material", "Matte Graphite & Brushed Metal"], ["Style", "Cyberpunk Gamer"], ["Lighting", "RGB Underglow"], ["Color", "Cyan Neon"], ["Gear", "Monitor + Tower + Headphones"], ["Vibe", "Late-Night Grind"]]))
	items.append(_rec("ornate_bookshelf", "Grand Mahogany Library Case", "table", "tables", "Legendary",
		"A carved mahogany library case with fluted gold columns, a gem-crowned arched pediment, a golden mantel clock, busts, and a glowing lantern.",
		[["Material", "Mahogany & Gold"], ["Style", "Baroque Library"], ["Crest", "Gem-Crowned Pediment"], ["Contents", "Tomes, Bust, Globe, Clock"], ["Feet", "Gilded Claw"], ["Theme", "Scholar's Hoard"], ["Vibe", "Old-Money Grandeur"]]))
	items.append(_rec("treasure_chest", "Overflowing Pirate Hoard", "table", "tables", "Epic",
		"A studded oak chest cracked open on gold hinges, spilling glowing coins, a crown jewel, scattered gems, and a rolled treasure map.",
		[["Material", "Aged Oak & Gold Straps"], ["Style", "Pirate Treasure"], ["Loot", "Coins, Gems & Crown Jewel"], ["Glow", "Golden Hoard"], ["Extra", "Treasure Map Scroll"], ["Vibe", "X Marks the Spot"]]))
	items.append(_rec("apothecary_cabinet", "Herbalist's Apothecary Chest", "table", "tables", "Rare",
		"A sage-teal chest of labelled brass-handled drawers under a glass display of glowing potions, with a stone mortar and a hanging herb bundle.",
		[["Material", "Painted Wood & Brass"], ["Style", "Cottagecore Alchemy"], ["Storage", "12 Labelled Drawers"], ["Display", "Glowing Potion Case"], ["Tools", "Mortar & Pestle"], ["Vibe", "Witchy & Cozy"]]))
	items.append(_rec("crystal_side_table", "Amethyst Geode Pedestal", "table", "tables", "Legendary",
		"A faceted amethyst slab on a glowing geode cluster rising from a gold base ring, topped by a hero crown gem and orbiting motes of light.",
		[["Material", "Amethyst Crystal & Gold"], ["Style", "Fantasy Luxe"], ["Color", "Violet, Rose & Cyan"], ["Glow", "Crystal Core & Motes"], ["Hero", "Faceted Crown Gem"], ["Vibe", "Enchanted Accent"]]))
	items.append(_rec("arcade_cabinet", "Neon Dreams Arcade Cabinet", "table", "tables", "Epic",
		"A retro upright arcade machine with a glowing marquee emblem, a lit game screen, a joystick and candy-colored buttons, and floor light-spill.",
		[["Material", "Lacquered Shell & Chrome"], ["Style", "80s Retro Arcade"], ["Color", "Purple & Hot Pink"], ["Lighting", "Marquee + Screen Glow"], ["Controls", "Joystick + 4 Buttons"], ["Vibe", "Insert Coin"]]))
	items.append(_rec("terrarium_table", "Living Biome Terrarium Table", "table", "tables", "Rare",
		"A glass-tank coffee table holding a tiny world of mossy mounds, a little tree, a glowing pond, luminous mushrooms, and drifting fireflies.",
		[["Material", "Glass, Brass & Warm Wood"], ["Style", "Living Terrarium"], ["Scene", "Moss, Tree, Pond & Cairn"], ["Glow", "Mushrooms & Fireflies"], ["Theme", "Miniature Forest"], ["Vibe", "Calm & Alive"]]))
	items.append(_rec("floating_shelf", "Floating Live-Edge Display Shelf", "table", "tables", "Uncommon",
		"A thick live-edge plank on hidden metal brackets with a brass lip and warm underglow, styled with a lit picture, books, a succulent, and a brass clock.",
		[["Material", "Live-Edge Wood & Brass"], ["Style", "Scandi Wall Decor"], ["Mount", "Hidden Floating Bracket"], ["Lighting", "Warm LED Underglow"], ["Props", "Frame, Books, Plant, Clock"], ["Vibe", "Curated & Airy"]]))

	# ---- LIGHTING (VerseCatalogLighting) ----
	items.append(_rec("art_deco_lamp", "Gatsby Torchiere", "lighting", "lighting", "Rare",
		"A glamorous Art-Deco floor lamp where a black-marble ziggurat, fluted brass column and golden sunburst fan cradle a glowing alabaster moon.",
		[["Style", "Art Deco"], ["Material", "Brass & Black Marble"], ["Accent", "Jade Cabochons"], ["Glow", "Warm Alabaster"], ["Theme", "Roaring Twenties"], ["Vibe", "Glamorous"]]))
	items.append(_rec("lantern_string", "Festival Lantern Garland", "lighting", "lighting", "Uncommon",
		"Five pleated rice-paper lanterns in candy festival colors swing from a catenary cord with fluttering bunting, gold crown caps and swaying tassels.",
		[["Style", "Festival Bunting"], ["Material", "Rice Paper & Bamboo"], ["Accent", "Brass Crown Caps"], ["Color", "Candy Multicolor"], ["Theme", "Garden Party"], ["Vibe", "Celebratory"]]))
	items.append(_rec("chandelier", "Crown of a Thousand Tears", "lighting", "lighting", "Legendary",
		"A multi-tier gold chandelier cascading with faceted crystal drops, six live candle-flames, ruby-set arms and a rising shimmer of golden glints.",
		[["Style", "Baroque Crystal"], ["Material", "Gold & Cut Crystal"], ["Gemstone", "Ruby"], ["Glow", "Candlelight"], ["Effect", "Rising Glints"], ["Vibe", "Opulent"]]))
	items.append(_rec("neon_sign", "HEY Neon", "lighting", "lighting", "Epic",
		"A buzzing cursive 'HEY' in hot-magenta neon tubing with a cyan swoosh, an amber star and a haloed glow off a brushed backboard with a humming transformer.",
		[["Style", "Retro Neon"], ["Material", "Glass Tube & Chrome"], ["Color", "Magenta & Cyan"], ["Glow", "Haloed Neon"], ["Theme", "Retro Bar"], ["Vibe", "Electric"]]))
	items.append(_rec("lava_lamp", "Groovy Goo Lamp", "lighting", "lighting", "Uncommon",
		"A retro lava lamp on a reeded chrome cone with peg feet and a glowing power knob, blobbing hot orange and magenta wax up its amber glass tower.",
		[["Style", "Retro Groovy"], ["Material", "Chrome & Amber Glass"], ["Color", "Orange & Magenta"], ["Effect", "Drifting Wax"], ["Theme", "Seventies"], ["Vibe", "Mellow"]]))
	items.append(_rec("campfire", "Cozy Campfire", "lighting", "lighting", "Common",
		"A ring of mossy river stones around crossed birch logs and a layered leaping flame that throws up a cheerful spray of dancing embers.",
		[["Style", "Rustic Outdoor"], ["Material", "River Stone & Birch"], ["Color", "Ember Orange"], ["Effect", "Rising Embers"], ["Theme", "Campsite"], ["Vibe", "Cozy"]]))
	items.append(_rec("fairy_jar", "Jar of Caught Stars", "lighting", "lighting", "Rare",
		"A corked mason jar tied with twine and a tiny brass keepsake tag, brimming with a slow swirl of golden fireflies twinkling in the glass.",
		[["Style", "Cottagecore"], ["Material", "Glass & Cork"], ["Accent", "Twine & Brass Tag"], ["Glow", "Firefly Gold"], ["Theme", "Enchanted Keepsake"], ["Vibe", "Whimsical"]]))
	items.append(_rec("street_lamp", "Old Town Gaslamp", "lighting", "lighting", "Common",
		"A storybook cast-iron gaslamp with a fluted post, C-scroll brackets, a six-sided glass lantern, a vented copper crown and a wee birdhouse perched on top.",
		[["Style", "Victorian Gaslamp"], ["Material", "Cast Iron & Copper"], ["Glow", "Warm Filament"], ["Detail", "Perched Birdhouse"], ["Theme", "Old Town"], ["Vibe", "Storybook"]]))
	items.append(_rec("mushroom_lamp", "Glowcap Grove", "lighting", "lighting", "Epic",
		"A mossy mound sprouting three bioluminescent toadstools with spotted caps and frilled glowing gills, a curled fern, glowing pebbles and drifting spores.",
		[["Style", "Enchanted Forest"], ["Material", "Moss & Glowcap"], ["Color", "Blue, Purple & Teal"], ["Glow", "Bioluminescent"], ["Effect", "Drifting Spores"], ["Vibe", "Magical"]]))

	# ---- WALL ART (VerseCatalogWallart) ----
	items.append(_rec("ornate_painting", "Golden Hour Masterpiece", "wallart", "wallart", "Epic",
		"A museum-grade gilt frame cradling a glowing sunset-over-the-lake oil scene, lit by its own brass picture light.",
		[["Material", "Carved Gilt Gold"], ["Style", "Classical Landscape"], ["Scene", "Sunset Over Hills"], ["Ornament", "Pearl Course + Ruby Rosettes"], ["Lighting", "Brass Picture Light"], ["Vibe", "Cozy Grandeur"]]))
	items.append(_rec("neon", "HEY! Neon Sign", "wallart", "wallart", "Rare",
		"A buzzing glass-tube HEY wordmark with a pink heart and one cheekily flickering dead segment, washing the wall in candy light.",
		[["Material", "Glass Neon Tube"], ["Style", "Retro Diner Sign"], ["Color", "Pink / Cyan / Amber"], ["Effect", "Colored Wall Wash"], ["Detail", "Flickering Dead Tube"], ["Vibe", "Late-Night Glow"]]))
	items.append(_rec("gilded_mirror", "Sunburst Oracle Mirror", "wallart", "wallart", "Legendary",
		"A double-ring sunburst of pointed gold rays surrounds a faceted mirror crowned with a giant ruby and a halo of glowing gems.",
		[["Material", "Polished Gold + Glass"], ["Style", "Baroque Sunburst"], ["Ornament", "Beaded Hoop + Gem Studs"], ["Gemstones", "Sapphire / Ruby Crown"], ["Effect", "Warm Wall Wash"], ["Vibe", "Royal Statement"]]))
	items.append(_rec("grand_clock", "Gilded Heirloom Clock", "wallart", "wallart", "Epic",
		"A stately beaded-bezel clock frozen at a friendly 10:10, with ornate hands, a gem center cap and a glowing swinging pendulum.",
		[["Material", "Gilt Brass + Porcelain"], ["Style", "Antique Mantel"], ["Detail", "Roman Cardinals + Minute Pips"], ["Ornament", "Beaded Bezel + Gem Crown"], ["Feature", "Glowing Pendulum Bob"], ["Vibe", "Timeless Heirloom"]]))
	items.append(_rec("pixel_screen", "8-Bit Sunrise CRT", "wallart", "wallart", "Uncommon",
		"A chunky beige CRT glowing with a pixel-art sunrise and a tiny hero sprite, complete with scanlines, vent slats and twiddly knobs.",
		[["Material", "Beige Plastic + Glow Pixels"], ["Style", "Retro Arcade CRT"], ["Scene", "Pixel Sunrise + Sprite"], ["Detail", "Scanlines + Brand Badge"], ["Vibe", "Nostalgic 8-Bit"]]))
	items.append(_rec("pennant", "Champions Felt Pennant", "wallart", "wallart", "Common",
		"A two-tone felt championship pennant on a turned wood dowel, sporting an appliqué star roundel, a stitched #1 and bobbled tassels.",
		[["Material", "Stitched Felt + Wood"], ["Style", "Sports Memorabilia"], ["Color", "Blue / Gold"], ["Detail", "Star Roundel + Tassels"], ["Vibe", "Team Spirit"]]))
	items.append(_rec("butterfly_display", "Jeweled Lepidoptera Case", "wallart", "wallart", "Rare",
		"A deep shadow-box of six jewel-bright butterflies pinned on linen behind glass, each with a chrome pin and tiny museum label.",
		[["Material", "Walnut Frame + Linen + Glass"], ["Style", "Victorian Entomology"], ["Specimens", "Six Butterflies"], ["Detail", "Chrome Pins + Brass Plate"], ["Color", "Iridescent Spectrum"], ["Vibe", "Curated Wonder"]]))
	items.append(_rec("vinyl_wall", "Now Playing Vinyl", "wallart", "wallart", "Uncommon",
		"A glossy grooved LP spinning over sunburst sleeve art, finished with a yellow 45 adapter and a gold now-playing plaque.",
		[["Material", "Vinyl + Brass Plaque"], ["Style", "Record Collector"], ["Detail", "Grooves + 45 Adapter"], ["Color", "Black / Sunburst Red"], ["Vibe", "Crate-Digger Cool"]]))
	items.append(_rec("holo_poster", "Holo Planetarium Panel", "wallart", "wallart", "Legendary",
		"A frameless acrylic sheet projecting a glowing wireframe planet, tilted orbit rings, gem nodes and drifting data motes from an edge-lit emitter rail.",
		[["Material", "Iridescent Acrylic + Hologram"], ["Style", "Future-Luxe Sci-Fi"], ["Scene", "Wireframe Planet + Orbits"], ["Effect", "Drifting Holo Motes"], ["Color", "Cyan / Violet Glow"], ["Vibe", "Levitating Hi-Tech"]]))

	# ---- PLANTS (VerseCatalogPlants) ----
	items.append(_rec("monstera", "Splitleaf Sweetheart", "plant", "plants", "Common",
		"A friendly split-leaf monstera fanning eight glossy paddles from a teal-banded deco pot.",
		[["Material", "Glazed Ceramic & Foliage"], ["Style", "Tropical Houseplant"], ["Color", "Emerald & Cream"], ["Theme", "Cozy Corner"], ["Vibe", "Leafy & Welcoming"], ["Leaves", "8 Paddles"]]))
	items.append(_rec("bonsai", "Whispering Elder Bonsai", "plant", "plants", "Uncommon",
		"A tiny ancient tree trained with spiralling copper wire on an oxblood-glaze tray, its cloud pads dusted with pink blossom.",
		[["Material", "Oxblood Glaze & Copper"], ["Style", "Cultivated Bonsai"], ["Color", "Sage & Copper"], ["Theme", "Zen Collector"], ["Vibe", "Calm & Refined"], ["Trim", "Copper Training Wire"]]))
	items.append(_rec("cherry_blossom", "Sakura Lantern Tree", "plant", "plants", "Rare",
		"A blushing sakura strung with warm fairy-lights on a brass-banded stone planter, raining soft petals.",
		[["Material", "Stone, Brass & Blossom"], ["Style", "Ornamental Sakura"], ["Color", "Blush Pink & Brass"], ["Theme", "Spring Festival"], ["Vibe", "Dreamy & Romantic"], ["Effect", "Drifting Petals & Fairy Glow"], ["Trim", "Polished Brass Band"]]))
	items.append(_rec("crystal_plant", "Aurora Geode Bloom", "plant", "plants", "Epic",
		"Bioluminescent gem shards rise from a gold-pronged obsidian geode beneath a hovering keystone jewel and a halo of sparks.",
		[["Material", "Obsidian, Gold & Crystal"], ["Style", "Bioluminescent Mineral"], ["Color", "Cyan, Violet & Gold"], ["Theme", "Arcane Geode"], ["Vibe", "Mystical & Radiant"], ["Effect", "Floating Gem & Rising Motes"], ["Light", "Emissive Glow"]]))
	items.append(_rec("cactus_trio", "Desert Buddy Trio", "plant", "plants", "Common",
		"A saguaro, a ribbed barrel and a prickly pear share one zigzag-painted pot, each crowned with a cheerful bloom.",
		[["Material", "Painted Terracotta & Cactus"], ["Style", "Desert Succulents"], ["Color", "Terracotta & Sage"], ["Theme", "Sunny Desk"], ["Vibe", "Cheerful & Spiky"], ["Count", "3 Cacti"]]))
	items.append(_rec("topiary_cat", "Hedge Cat of the Garden", "plant", "plants", "Rare",
		"A sculpted hedge shaped into a sitting cat with glowing eyes, wearing a real gold collar, amber bell and a gemstone charm.",
		[["Material", "Clipped Hedge, Stone & Gold"], ["Style", "Whimsical Topiary"], ["Color", "Forest Green & Gold"], ["Theme", "Garden Whimsy"], ["Vibe", "Playful & Regal"], ["Effect", "Glowing Eyes"], ["Trim", "Gold Collar & Gem Charm"]]))
	items.append(_rec("hanging_fern", "Boho Cascade Fern", "plant", "plants", "Uncommon",
		"A lush trailing fern spilling from a beaded macramé hanger on a polished brass ring, dotted with coral spore-bells.",
		[["Material", "Macramé, Brass & Fern"], ["Style", "Boho Hanging Planter"], ["Color", "Green & Wheat"], ["Theme", "Cozy Boho"], ["Vibe", "Relaxed & Trailing"], ["Mount", "Hangs from Hook"]]))
	items.append(_rec("sunflower_patch", "Sunny Day Crate", "plant", "plants", "Common",
		"A trio of beaming sunflowers with double petal rings nod over a slatted wooden crate while a little bee hovers by.",
		[["Material", "Wood Crate & Sunflowers"], ["Style", "Rustic Cottage Garden"], ["Color", "Golden Yellow & Pine"], ["Theme", "Summer Cheer"], ["Vibe", "Happy & Warm"], ["Detail", "Hovering Bee"]]))
	items.append(_rec("potted_palm", "Tradewind Brass Palm", "plant", "plants", "Uncommon",
		"A breezy fan palm arches from a brass-banded woven basket, hiding a pair of coconuts beneath its crown.",
		[["Material", "Woven Basket, Brass & Palm"], ["Style", "Tropical Statement"], ["Color", "Jungle Green & Brass"], ["Theme", "Resort Corner"], ["Vibe", "Breezy & Lush"], ["Trim", "Polished Brass Bands"]]))
	items.append(_rec("carnivorous", "Nectar Trap Bog", "plant", "plants", "Epic",
		"Toothy flytraps and a glowing pitcher rise from a gold-rimmed mossy bog, each lure beaded with a faceted amber dewdrop as a lured fly hovers near.",
		[["Material", "Moss, Gold & Carnivore"], ["Style", "Carnivorous Bog"], ["Color", "Bog Green, Crimson & Gold"], ["Theme", "Curious Predator"], ["Vibe", "Cheeky & Dangerous"], ["Effect", "Lure Glow & Nectar Gems"], ["Detail", "Lured Fly"]]))
	items.append(_rec("zen_garden", "Imperial Zen Sanctuary", "plant", "plants", "Legendary",
		"A gold-inlaid raked-sand tray with balancing stones, a maple sprig, a gem-set gilded lantern, a glowing koi chip and a hovering jade relic orb.",
		[["Material", "Dark Wood, Gold & Jade"], ["Style", "Imperial Zen Garden"], ["Color", "Gold, Jade & Sand"], ["Theme", "Serene Sanctuary"], ["Vibe", "Tranquil & Opulent"], ["Effect", "Floating Jade Orb & Lantern Glow"], ["Trim", "Gold Inlay & Gemstones"], ["Light", "Dual Omni Glow"]]))

	# ---- DECOR (VerseCatalogDecor) ----
	items.append(_rec("rug", "Madder Medallion Rug", "decor", "decor", "Common",
		"A plush hand-knotted wool rug whose radiating star medallion makes any spot on the floor feel like home.",
		[["Material", "Wool"], ["Style", "Persian Medallion"], ["Palette", "Madder Red & Indigo"], ["Pattern", "Radiating Star"], ["Vibe", "Cozy"], ["Finish", "Fringed"]]))
	items.append(_rec("vase", "Porcelain Posy Vase", "decor", "decor", "Uncommon",
		"A glossy teal-glaze vase with gold filigree handles, brimming with a freshly-cut spring posy.",
		[["Material", "Glazed Porcelain"], ["Trim", "Gold Filigree"], ["Color", "Teal & Gold"], ["Style", "Ornate Antique"], ["Bouquet", "Five Blooms"], ["Vibe", "Fresh"]]))
	items.append(_rec("statue", "Golden Hero Monument", "decor", "decor", "Epic",
		"A proud civic statue of your robot kind cast in solid gold, gem-set and glowing, atop a laureled marble plinth.",
		[["Material", "Solid Gold"], ["Base", "Banded Marble"], ["Gems", "Sapphire"], ["Pose", "Triumphant"], ["Theme", "Heroic"], ["Vibe", "Monumental"]]))
	items.append(_rec("fountain", "Grand Cascade Fountain", "decor", "decor", "Legendary",
		"A three-tier stone fountain crowned with a ruby finial, ringed by golden dolphin spouts and lily pads, pouring lit, sparkling water.",
		[["Material", "Carved Stone & Gold"], ["Tiers", "Three"], ["Gems", "Ruby Inlays"], ["Spouts", "Golden Dolphins"], ["Feature", "Flowing Lit Water"], ["Theme", "Royal Garden"], ["Vibe", "Centerpiece"]]))
	items.append(_rec("snowglobe", "Winter Village Globe", "decor", "decor", "Rare",
		"A whole snowy hamlet under glass: a lit cottage, a pine, a top-hatted snowman and a glowing lamppost, dusted by drifting snow.",
		[["Material", "Carved Wood & Glass"], ["Scene", "Winter Village"], ["Effect", "Drifting Snow"], ["Trim", "Gold Bands"], ["Theme", "Festive"], ["Vibe", "Whimsical"]]))
	items.append(_rec("gramophone", "Brass Morning-Glory Gramophone", "decor", "decor", "Rare",
		"A vintage phonograph with a great fluted brass horn and a spinning record, trailing glowing music notes into the air.",
		[["Material", "Polished Brass & Wood"], ["Horn", "Fluted Morning-Glory"], ["Era", "Antique"], ["Feature", "Spinning Record"], ["Effect", "Floating Notes"], ["Vibe", "Nostalgic"]]))
	items.append(_rec("telescope", "Observatory Refractor", "decor", "decor", "Epic",
		"A polished brass refractor on a wooden tripod, star-map ring set, aimed at a tiny glowing constellation only it can see.",
		[["Material", "Brass & Hardwood"], ["Type", "Refractor"], ["Mount", "Tripod & Counterweight"], ["Feature", "Star-Map Ring"], ["Theme", "Explorer"], ["Vibe", "Wondrous"]]))
	items.append(_rec("aquarium", "Living Reef Aquarium", "decor", "decor", "Epic",
		"A lit reef tank alive with neon fish, branching coral, a tiny castle and a curtain of rising bubbles, all on a wood cabinet.",
		[["Material", "Glass & Wood Cabinet"], ["Inhabitants", "Neon Fish"], ["Scenery", "Coral & Castle"], ["Effect", "Rising Bubbles"], ["Lighting", "Hood Glow"], ["Vibe", "Living"]]))
	items.append(_rec("crystal", "Hovering Arcane Crystal", "decor", "decor", "Legendary",
		"A faceted arcane gem that hovers above a runed pedestal, ringed by orbiting shards and floating glyphs in a beam of energy.",
		[["Material", "Faceted Crystal"], ["Base", "Runed Pedestal"], ["Effect", "Levitation & Glow"], ["Aura", "Orbiting Shards"], ["Theme", "Arcane"], ["Vibe", "Mystical"]]))
	items.append(_rec("balloons", "Party Balloon Bunch", "decor", "decor", "Uncommon",
		"Five glossy jewel-tone helium balloons led by a gold foil star, curled ribbons tied to a wrapped gift, with drifting confetti.",
		[["Material", "Glossy Latex & Gold Foil"], ["Count", "Five"], ["Highlight", "Foil Star Balloon"], ["Anchor", "Wrapped Gift"], ["Effect", "Confetti"], ["Vibe", "Celebratory"]]))
	items.append(_rec("trophy", "Champions' Cup", "decor", "decor", "Rare",
		"A gleaming gold cup with looping handles and a laurel wreath, set with emeralds on an obsidian base, crowned by a bursting star.",
		[["Material", "Gold & Obsidian"], ["Gems", "Emerald"], ["Detail", "Laurel Wreath"], ["Crown", "Glowing Star"], ["Theme", "Victory"], ["Vibe", "Triumphant"]]))

	return items


## Build a renderable Node3D for `id` by dispatching to the owning module's
## static builder. Returns null for an unknown id (caller should guard). Each
## module is named by its class_name so this resolves through the class cache.
static func build(id: String) -> Node3D:
	match id:
		# hats
		"party_hat": return VerseCatalogHats.build_party_hat()
		"top_hat": return VerseCatalogHats.build_top_hat()
		"propeller_cap": return VerseCatalogHats.build_propeller_cap()
		"cat_ears": return VerseCatalogHats.build_cat_ears()
		"viking_helmet": return VerseCatalogHats.build_viking_helmet()
		"headphones": return VerseCatalogHats.build_headphones()
		"flower_crown": return VerseCatalogHats.build_flower_crown()
		"wizard_hat": return VerseCatalogHats.build_wizard_hat()
		"crown": return VerseCatalogHats.build_crown()
		"halo": return VerseCatalogHats.build_halo()
		"astronaut_helmet": return VerseCatalogHats.build_astronaut_helmet()
		"flame_crown": return VerseCatalogHats.build_flame_crown()
		# seating
		"velvet_armchair": return VerseCatalogSeating.build_velvet_armchair()
		"royal_throne": return VerseCatalogSeating.build_royal_throne()
		"gaming_chair": return VerseCatalogSeating.build_gaming_chair()
		"beanbag": return VerseCatalogSeating.build_beanbag()
		"carved_dining_chair": return VerseCatalogSeating.build_carved_dining_chair()
		"mushroom_stool": return VerseCatalogSeating.build_mushroom_stool()
		"swing_seat": return VerseCatalogSeating.build_swing_seat()
		"chesterfield_sofa": return VerseCatalogSeating.build_chesterfield_sofa()
		"hammock": return VerseCatalogSeating.build_hammock()
		"cloud_sofa": return VerseCatalogSeating.build_cloud_sofa()
		# tables
		"marble_coffee_table": return VerseCatalogTables.build_marble_coffee_table()
		"oak_dining_table": return VerseCatalogTables.build_oak_dining_table()
		"glowing_desk": return VerseCatalogTables.build_glowing_desk()
		"ornate_bookshelf": return VerseCatalogTables.build_ornate_bookshelf()
		"treasure_chest": return VerseCatalogTables.build_treasure_chest()
		"apothecary_cabinet": return VerseCatalogTables.build_apothecary_cabinet()
		"crystal_side_table": return VerseCatalogTables.build_crystal_side_table()
		"arcade_cabinet": return VerseCatalogTables.build_arcade_cabinet()
		"terrarium_table": return VerseCatalogTables.build_terrarium_table()
		"floating_shelf": return VerseCatalogTables.build_floating_shelf()
		# lighting
		"art_deco_lamp": return VerseCatalogLighting.build_art_deco_lamp()
		"lantern_string": return VerseCatalogLighting.build_lantern_string()
		"chandelier": return VerseCatalogLighting.build_chandelier()
		"neon_sign": return VerseCatalogLighting.build_neon_sign()
		"lava_lamp": return VerseCatalogLighting.build_lava_lamp()
		"campfire": return VerseCatalogLighting.build_campfire()
		"fairy_jar": return VerseCatalogLighting.build_fairy_jar()
		"street_lamp": return VerseCatalogLighting.build_street_lamp()
		"mushroom_lamp": return VerseCatalogLighting.build_mushroom_lamp()
		# wall art
		"ornate_painting": return VerseCatalogWallart.build_ornate_painting()
		"neon": return VerseCatalogWallart.build_neon()
		"gilded_mirror": return VerseCatalogWallart.build_gilded_mirror()
		"grand_clock": return VerseCatalogWallart.build_grand_clock()
		"pixel_screen": return VerseCatalogWallart.build_pixel_screen()
		"pennant": return VerseCatalogWallart.build_pennant()
		"butterfly_display": return VerseCatalogWallart.build_butterfly_display()
		"vinyl_wall": return VerseCatalogWallart.build_vinyl_wall()
		"holo_poster": return VerseCatalogWallart.build_holo_poster()
		# plants
		"monstera": return VerseCatalogPlants.build_monstera()
		"bonsai": return VerseCatalogPlants.build_bonsai()
		"cherry_blossom": return VerseCatalogPlants.build_cherry_blossom()
		"crystal_plant": return VerseCatalogPlants.build_crystal_plant()
		"cactus_trio": return VerseCatalogPlants.build_cactus_trio()
		"topiary_cat": return VerseCatalogPlants.build_topiary_cat()
		"hanging_fern": return VerseCatalogPlants.build_hanging_fern()
		"sunflower_patch": return VerseCatalogPlants.build_sunflower_patch()
		"potted_palm": return VerseCatalogPlants.build_potted_palm()
		"carnivorous": return VerseCatalogPlants.build_carnivorous()
		"zen_garden": return VerseCatalogPlants.build_zen_garden()
		# decor
		"rug": return VerseCatalogDecor.build_rug()
		"vase": return VerseCatalogDecor.build_vase()
		"statue": return VerseCatalogDecor.build_statue()
		"fountain": return VerseCatalogDecor.build_fountain()
		"snowglobe": return VerseCatalogDecor.build_snowglobe()
		"gramophone": return VerseCatalogDecor.build_gramophone()
		"telescope": return VerseCatalogDecor.build_telescope()
		"aquarium": return VerseCatalogDecor.build_aquarium()
		"crystal": return VerseCatalogDecor.build_crystal()
		"balloons": return VerseCatalogDecor.build_balloons()
		"trophy": return VerseCatalogDecor.build_trophy()
	push_warning("VerseCatalog.build: unknown id '%s'" % id)
	return null


## Color for a rarity (falls back to white for an unknown tier).
static func rarity_color(rarity: String) -> Color:
	return RARITY_COLOR.get(rarity, Color(1, 1, 1))


## Internal: assemble one full NFT-trait record. `traits` is an Array of
## [trait_type, value] string pairs -> [{trait_type, value}, ...].
static func _rec(id: String, name_: String, kind: String, theme: String, rarity: String, description: String, traits: Array) -> Dictionary:
	var attributes: Array = []
	for pair in traits:
		attributes.append({"trait_type": pair[0], "value": pair[1]})
	return {
		"id": id,
		"name": name_,
		"kind": kind,
		"theme": theme,
		"rarity": rarity,
		"description": description,
		"attributes": attributes,
	}
