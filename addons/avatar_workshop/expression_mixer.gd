extends RefCounted


static func mix(weights: Array, bindings: Array) -> Dictionary:
	var result := {}
	for channel in bindings.size():
		for binding: Array in bindings[channel]:
			var target := binding.slice(0, 2)
			result[target] = result.get(target, 0.0) + float(weights[channel]) * float(binding[2])
	for target: Array in result:
		result[target] = clampf(result[target], 0.0, 1.0)
	return result
