// Called on Quill's speech worker. Apple receives the captured PCM buffer
// only after release and is required to recognize it on this Mac.
#import <AVFoundation/AVFoundation.h>
#import <Speech/Speech.h>

static SFSpeechRecognizer *recognizer(const char *language) {
    NSString *tag = [NSString stringWithUTF8String:language];
    if ([tag isEqualToString:@"auto"] || tag.length == 0)
        return [[SFSpeechRecognizer alloc] init];
    NSLocale *locale = [NSLocale localeWithLocaleIdentifier:tag];
    return [[SFSpeechRecognizer alloc] initWithLocale:locale];
}

bool quill_apple_speech_supported(const char *language) {
    @autoreleasepool {
        return recognizer(language).supportsOnDeviceRecognition;
    }
}

int quill_apple_transcribe(const char *language, const float *samples,
                           uint32_t count, char **output) {
    @autoreleasepool {
        SFSpeechRecognizer *speech = recognizer(language);
        if (!speech.supportsOnDeviceRecognition) return 1;
        if (!speech.available) return 2;
        speech.queue = [[NSOperationQueue alloc] init];
        speech.queue.maxConcurrentOperationCount = 1;
        SFSpeechAudioBufferRecognitionRequest *request = [SFSpeechAudioBufferRecognitionRequest new];
        request.requiresOnDeviceRecognition = YES;
        request.shouldReportPartialResults = YES;
        request.taskHint = SFSpeechRecognitionTaskHintDictation;
        AVAudioFormat *format = [[AVAudioFormat alloc] initStandardFormatWithSampleRate:16000 channels:1];
        AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:count];
        if (!buffer.floatChannelData) return 3;
        buffer.frameLength = count;
        memcpy(buffer.floatChannelData[0], samples, (size_t)count * sizeof(float));
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        // Apple can seal a phrase during a pause and start a fresh transcript.
        // Its terminal callback may repeat or refine the last sealed phrase.
        NSMutableArray<NSString *> *phrases = [NSMutableArray array];
        __block NSTimeInterval lastStart = -1;
        __block BOOL success = NO;
        __block BOOL completed = NO;
        SFSpeechRecognitionTask *task = [speech recognitionTaskWithRequest:request resultHandler:
            ^(SFSpeechRecognitionResult *result, NSError *error) {
                if (completed) return;
                SFTranscription *transcript = result.bestTranscription;
                if ((result.speechRecognitionMetadata || result.isFinal) && transcript.segments.count && transcript.formattedString.length) {
                    NSTimeInterval start = transcript.segments.firstObject.timestamp;
                    if (phrases.count && start == lastStart) {
                        phrases[phrases.count - 1] = transcript.formattedString;
                    } else {
                        [phrases addObject:transcript.formattedString];
                        lastStart = start;
                    }
                }
                if (!result.isFinal && !error) return;
                completed = YES;
                success = !error || ([error.domain isEqualToString:@"kAFAssistantErrorDomain"] && error.code == 1110);
                dispatch_semaphore_signal(done);
            }];
        [request appendAudioPCMBuffer:buffer];
        [request endAudio];
        if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 90LL * NSEC_PER_SEC))) {
            [task cancel];
            return 4;
        }
        if (!success) return 3;
        NSString *text = [phrases componentsJoinedByString:@" "];
        *output = strdup(text.UTF8String);
        return *output ? 0 : 3;
    }
}

void quill_apple_free_text(char *text) { free(text); }
