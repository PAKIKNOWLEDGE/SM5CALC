#include "MinaCalc.h"

#include <vector>

struct MinaNoteC {
	unsigned int notes;
	float row_time;
};

static void
fill_zero(float* out)
{
	for (int i = 0; i < 14 * 8; ++i) {
		out[i] = 0.F;
	}
}

extern "C" int
mina_calc_version()
{
	return GetCalcVersion();
}

extern "C" int
mina_sd_all_rates(const MinaNoteC* notes,
				  int count,
				  unsigned keycount,
				  float* out)
{
	if (out == nullptr) {
		return -1;
	}
	if (notes == nullptr || count <= 1 || keycount == 0) {
		fill_zero(out);
		return 0;
	}

	try {
		std::vector<NoteInfo> rows;
		rows.reserve(static_cast<size_t>(count));
		for (int i = 0; i < count; ++i) {
			rows.push_back(NoteInfo{ notes[i].notes, notes[i].row_time });
		}

		// One calc for the process. Callers serialize access.
		static Calc calc;
		const MinaSD result = MinaSDCalc(rows, keycount, &calc);
		for (int rate = 0; rate < 14; ++rate) {
			for (int skill = 0; skill < 8; ++skill) {
				float value = 0.F;
				if (rate < static_cast<int>(result.size()) &&
					skill < static_cast<int>(result[rate].size())) {
					value = result[rate][static_cast<size_t>(skill)];
				}
				out[rate * 8 + skill] = value;
			}
		}
		return 0;
	} catch (...) {
		fill_zero(out);
		return -1;
	}
}
