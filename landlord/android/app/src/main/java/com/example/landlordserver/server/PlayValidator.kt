package com.example.landlordserver.server

private enum class ComboKind {
    Rocket,
    Bomb,
    Single,
    Pair,
    Triple,
    TripleSingle,
    TriplePair,
    Straight,
    StraightPairs,
    Plane,
    PlaneWithSingles,
    PlaneWithPairs,
    FourWithTwoSingles,
    FourWithTwoPairs,
}

private data class Combo(
    val kind: ComboKind,
    val mainRank: Int,
    val sequenceLen: Int,
)

fun validatePlay(game: LandlordGame, position: Int, cards: List<Int>): Boolean {
    if (game.phase != LandlordPhase.PLAY || game.currentPosition != position) return false
    val hand = game.hands[position] ?: return false
    if (!cardsInHand(cards, hand)) return false

    if (cards.isEmpty()) {
        return game.lastPlay.isNotEmpty() && game.lastPlayPosition != position
    }

    val combo = classify(cards) ?: return false
    if (game.lastPlay.isEmpty() || game.lastPlayPosition == position) return true
    val previous = classify(game.lastPlay) ?: return true
    return canBeat(combo, previous)
}

private fun canBeat(curr: Combo, prev: Combo): Boolean {
    if (curr.kind == ComboKind.Rocket) return prev.kind != ComboKind.Rocket
    if (curr.kind == ComboKind.Bomb) {
        return when (prev.kind) {
            ComboKind.Rocket -> false
            ComboKind.Bomb -> curr.mainRank > prev.mainRank
            else -> true
        }
    }
    if (prev.kind == ComboKind.Rocket || prev.kind == ComboKind.Bomb) return false
    return curr.kind == prev.kind && curr.sequenceLen == prev.sequenceLen && curr.mainRank > prev.mainRank
}

private fun cardsInHand(cards: List<Int>, hand: List<Int>): Boolean {
    val counts = hand.groupingBy { it }.eachCount().toMutableMap()
    for (card in cards) {
        if (card !in 1..54) return false
        val count = counts[card] ?: return false
        if (count <= 0) return false
        counts[card] = count - 1
    }
    return true
}

private fun classify(cards: List<Int>): Combo? {
    if (cards.isEmpty()) return null
    if (cards.any { it !in 1..54 }) return null

    val len = cards.size
    val counts = rankCounts(cards)
    val groups = counts.entries.sortedBy { it.key }.map { it.key to it.value }

    if (len == 2 && counts[16] == 1 && counts[17] == 1) return Combo(ComboKind.Rocket, 17, 1)
    if (len == 4 && groups.size == 1 && groups[0].second == 4) return Combo(ComboKind.Bomb, groups[0].first, 1)
    if (len == 1) return Combo(ComboKind.Single, groups[0].first, 1)
    if (len == 2 && groups.size == 1 && groups[0].second == 2) return Combo(ComboKind.Pair, groups[0].first, 1)
    if (len == 3 && groups.size == 1 && groups[0].second == 3) return Combo(ComboKind.Triple, groups[0].first, 1)
    if (len == 4 && groups.size == 2) {
        val triple = groups.firstOrNull { it.second == 3 } ?: return null
        return Combo(ComboKind.TripleSingle, triple.first, 1)
    }
    if (len == 5 && groups.size == 2) {
        val triple = groups.firstOrNull { it.second == 3 }
        if (triple != null && groups.any { it.second == 2 }) return Combo(ComboKind.TriplePair, triple.first, 1)
    }

    val singleRanks = groups.filter { it.second == 1 }.map { it.first }
    if (len >= 5 && singleRanks.size == len && singleRanks.all { it < 15 } && isConsecutive(singleRanks)) {
        return Combo(ComboKind.Straight, singleRanks.last(), len)
    }

    val pairRanks = groups.filter { it.second == 2 }.map { it.first }
    if (len >= 6 && len % 2 == 0 && pairRanks.size * 2 == len && pairRanks.all { it < 15 } && isConsecutive(pairRanks)) {
        return Combo(ComboKind.StraightPairs, pairRanks.last(), pairRanks.size)
    }

    val tripleRanks = groups.filter { it.second == 3 }.map { it.first }
    if (tripleRanks.size >= 2 && tripleRanks.all { it < 15 } && isConsecutive(tripleRanks)) {
        val n = tripleRanks.size
        if (len == n * 3) return Combo(ComboKind.Plane, tripleRanks.last(), n)
        if (len == n * 4) {
            val wings = groups.count { it.second == 1 && it.first !in tripleRanks }
            if (wings == n) return Combo(ComboKind.PlaneWithSingles, tripleRanks.last(), n)
        }
        if (len == n * 5) {
            val wingPairs = groups.count { it.second == 2 && it.first !in tripleRanks }
            if (wingPairs == n) return Combo(ComboKind.PlaneWithPairs, tripleRanks.last(), n)
        }
    }

    if (len == 6) {
        val four = groups.firstOrNull { it.second == 4 }
        if (four != null) return Combo(ComboKind.FourWithTwoSingles, four.first, 1)
    }
    if (len == 8) {
        val four = groups.firstOrNull { it.second == 4 }
        if (four != null && groups.count { it.second == 2 } == 2) return Combo(ComboKind.FourWithTwoPairs, four.first, 1)
    }

    return null
}

private fun rankCounts(cards: List<Int>): Map<Int, Int> =
    cards.groupingBy { cardRank(it) }.eachCount()

private fun cardRank(card: Int): Int = when (card) {
    53 -> 16
    54 -> 17
    else -> ((card - 1) % 13) + 3
}

private fun isConsecutive(ranks: List<Int>): Boolean =
    ranks.isNotEmpty() && ranks.windowed(2).all { (a, b) -> b == a + 1 }
